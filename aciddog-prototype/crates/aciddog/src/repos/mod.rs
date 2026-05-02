use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use tracing::warn;

use crate::config::Repository;

#[derive(Debug, Clone)]
pub struct Worktree {
    pub path: PathBuf,
    pub branch: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RepoState {
    pub id: String,
    pub url: String,
    pub path: PathBuf,
    pub default_branch: Option<String>,
    pub worktrees: Vec<Worktree>,
    pub last_worktree_scan: Option<Instant>,
}

#[derive(Debug, thiserror::Error)]
pub enum RepoStartupError {
    #[error("repository `{id}` clone failed (url={url}, path={path}): {stderr}", path = path.display())]
    CloneFailed {
        id: String,
        url: String,
        path: PathBuf,
        stderr: String,
    },

    #[error(
        "repository `{id}` origin url drifted: declared={declared}, observed={observed}, path={path}",
        path = path.display()
    )]
    UrlDrift {
        id: String,
        declared: String,
        observed: String,
        path: PathBuf,
    },

    #[error("repository `{id}` path exists but is not a git repository: {path}", path = path.display())]
    NotAGitRepo { id: String, path: PathBuf },

    #[error("repository `{id}` path is invalid: {path} ({reason})", path = path.display())]
    BadPath {
        id: String,
        path: PathBuf,
        reason: String,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum RepoRefreshError {
    #[error("repository `{id}` worktree refresh failed: {stderr}")]
    GitFailed { id: String, stderr: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffOutcome {
    NoForwardChanges,
    Diff {
        lines: Vec<String>,
        truncated_at: Option<usize>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("repository diff command failed: {command}: {stderr}")]
pub struct RepoDiffError {
    pub command: String,
    pub stderr: String,
}

pub fn ensure_repo(decl: &Repository, base_dir: &Path) -> Result<RepoState, RepoStartupError> {
    let path = base_dir.join(&decl.id);
    if path.as_os_str().is_empty() {
        return Err(RepoStartupError::BadPath {
            id: decl.id.clone(),
            path,
            reason: "empty path".to_string(),
        });
    }

    if !path.exists() {
        let output = Command::new("git")
            .arg("clone")
            .arg(&decl.url)
            .arg(&path)
            .output()
            .map_err(|e| RepoStartupError::CloneFailed {
                id: decl.id.clone(),
                url: decl.url.clone(),
                path: path.clone(),
                stderr: e.to_string(),
            })?;
        if !output.status.success() {
            return Err(RepoStartupError::CloneFailed {
                id: decl.id.clone(),
                url: decl.url.clone(),
                path,
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            });
        }
    } else {
        let output = Command::new("git")
            .arg("-C")
            .arg(&path)
            .arg("rev-parse")
            .arg("--git-dir")
            .output()
            .map_err(|e| RepoStartupError::BadPath {
                id: decl.id.clone(),
                path: path.clone(),
                reason: format!("failed to run git rev-parse: {e}"),
            })?;
        if !output.status.success() {
            return Err(RepoStartupError::NotAGitRepo {
                id: decl.id.clone(),
                path,
            });
        }

        let output = Command::new("git")
            .arg("-C")
            .arg(&path)
            .arg("remote")
            .arg("get-url")
            .arg("origin")
            .output()
            .map_err(|e| RepoStartupError::UrlDrift {
                id: decl.id.clone(),
                declared: decl.url.clone(),
                observed: e.to_string(),
                path: path.clone(),
            })?;
        let observed = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !output.status.success() || observed != decl.url {
            return Err(RepoStartupError::UrlDrift {
                id: decl.id.clone(),
                declared: decl.url.clone(),
                observed,
                path,
            });
        }
    }

    let default_branch = match &decl.branch {
        Some(branch) => Some(branch.clone()),
        None => probe_default_branch(&decl.id, &path),
    };

    Ok(RepoState {
        id: decl.id.clone(),
        url: decl.url.clone(),
        path,
        default_branch,
        worktrees: vec![],
        last_worktree_scan: None,
    })
}

pub fn refresh_worktrees(state: &mut RepoState) -> Result<(), RepoRefreshError> {
    let output = Command::new("git")
        .arg("-C")
        .arg(&state.path)
        .arg("worktree")
        .arg("list")
        .arg("--porcelain")
        .output()
        .map_err(|e| RepoRefreshError::GitFailed {
            id: state.id.clone(),
            stderr: e.to_string(),
        })?;

    if !output.status.success() {
        return Err(RepoRefreshError::GitFailed {
            id: state.id.clone(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }

    let mut worktrees = parse_worktree_porcelain(&String::from_utf8_lossy(&output.stdout));
    sort_worktrees(&mut worktrees, state.default_branch.as_deref());
    state.worktrees = worktrees;
    state.last_worktree_scan = Some(Instant::now());
    Ok(())
}

pub fn compute_branch_diff(repo: &RepoState, branch: &str) -> Result<DiffOutcome, RepoDiffError> {
    let Some(default_branch) = repo.default_branch.as_deref() else {
        return Err(RepoDiffError {
            command: diff_command_string(&repo.path, "<default>", branch),
            stderr: "repo has no resolved default branch".to_string(),
        });
    };

    if branch == default_branch {
        return Ok(DiffOutcome::NoForwardChanges);
    }

    let command = diff_command_string(&repo.path, default_branch, branch);
    let output = Command::new("git")
        .arg("-C")
        .arg(&repo.path)
        .arg("diff")
        .arg("--no-color")
        .arg(format!("{default_branch}...{branch}"))
        .output()
        .map_err(|e| RepoDiffError {
            command: command.clone(),
            stderr: e.to_string(),
        })?;

    if !output.status.success() {
        return Err(RepoDiffError {
            command,
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    if stdout.is_empty() {
        return Ok(DiffOutcome::NoForwardChanges);
    }

    let (lines, truncated_at) = truncate_diff_lines(&stdout, 10_000);
    Ok(DiffOutcome::Diff {
        lines,
        truncated_at,
    })
}

fn diff_command_string(path: &Path, default_branch: &str, branch: &str) -> String {
    format!(
        "git -C {} diff --no-color {default_branch}...{branch}",
        path.display()
    )
}

pub(crate) fn truncate_diff_lines(stdout: &str, cap: usize) -> (Vec<String>, Option<usize>) {
    let mut lines: Vec<String> = stdout.split('\n').map(str::to_string).collect();
    let original_len = lines.len();
    if original_len > cap {
        lines.truncate(cap);
        (lines, Some(original_len))
    } else {
        (lines, None)
    }
}

pub(crate) fn parse_worktree_porcelain(raw: &str) -> Vec<Worktree> {
    let mut worktrees = Vec::new();
    let mut block = Vec::new();

    for line in raw.lines() {
        if line.trim().is_empty() {
            push_worktree_block(&mut worktrees, &block);
            block.clear();
        } else {
            block.push(line);
        }
    }
    push_worktree_block(&mut worktrees, &block);

    worktrees
}

fn push_worktree_block(worktrees: &mut Vec<Worktree>, block: &[&str]) {
    if block.is_empty() {
        return;
    }

    let Some(path) = block
        .iter()
        .find_map(|line| line.strip_prefix("worktree "))
        .map(PathBuf::from)
    else {
        return;
    };
    let branch = block.iter().find_map(|line| {
        line.strip_prefix("branch refs/heads/")
            .map(ToString::to_string)
    });

    worktrees.push(Worktree { path, branch });
}

fn sort_worktrees(worktrees: &mut [Worktree], default_branch: Option<&str>) {
    worktrees.sort_by_key(|worktree| {
        let bucket = match worktree.branch.as_deref() {
            Some(branch) if Some(branch) == default_branch => 0,
            Some(_) => 1,
            None => 2,
        };
        let key = worktree.branch.as_deref().map_or_else(
            || worktree.path.to_string_lossy().into_owned(),
            str::to_string,
        );
        let path = worktree.path.to_string_lossy().into_owned();
        (bucket, key, path)
    });
}

fn probe_default_branch(id: &str, path: &Path) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .arg("symbolic-ref")
        .arg("--short")
        .arg("refs/remotes/origin/HEAD")
        .output();
    match output {
        Ok(output) if output.status.success() => {
            let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
            Some(
                branch
                    .strip_prefix("origin/")
                    .unwrap_or(&branch)
                    .to_string(),
            )
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!(
                repository = %id,
                path = %path.display(),
                stderr = %stderr.trim(),
                "failed to probe repository default branch"
            );
            None
        }
        Err(e) => {
            warn!(
                repository = %id,
                path = %path.display(),
                error = %e,
                "failed to probe repository default branch"
            );
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::process::Command;

    use tempfile::TempDir;

    use super::*;

    fn repository(id: &str, url: String) -> Repository {
        Repository {
            id: id.to_string(),
            url,
            branch: None,
        }
    }

    fn init_bare_remote() -> (TempDir, String) {
        let remote = tempfile::tempdir().unwrap();
        let output = Command::new("git")
            .arg("init")
            .arg("--bare")
            .arg(remote.path())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git init --bare failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let url = format!("file://{}", remote.path().display());
        (remote, url)
    }

    fn git_stdout(path: &Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .arg("-C")
            .arg(path)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    fn repo_state(path: PathBuf, default_branch: Option<&str>) -> RepoState {
        RepoState {
            id: "repo".to_string(),
            url: "file:///repo".to_string(),
            path,
            default_branch: default_branch.map(str::to_string),
            worktrees: vec![],
            last_worktree_scan: None,
        }
    }

    #[test]
    fn parses_single_main_worktree() {
        let worktrees = parse_worktree_porcelain(
            "worktree /repo\nHEAD abcdef0123456789abcdef0123456789abcdef01\nbranch refs/heads/main\n",
        );

        assert_eq!(worktrees.len(), 1);
        assert_eq!(worktrees[0].path, PathBuf::from("/repo"));
        assert_eq!(worktrees[0].branch.as_deref(), Some("main"));
    }

    #[test]
    fn parses_main_and_linked_worktrees() {
        let worktrees = parse_worktree_porcelain(
            "worktree /repo\nHEAD abcdef0123456789abcdef0123456789abcdef01\nbranch refs/heads/main\n\nworktree /repo-wt/feature-a\nHEAD abcdef0123456789abcdef0123456789abcdef02\nbranch refs/heads/feature/a\n\nworktree /repo-wt/bugfix\nHEAD abcdef0123456789abcdef0123456789abcdef03\nbranch refs/heads/bugfix\n",
        );

        assert_eq!(worktrees.len(), 3);
        assert_eq!(worktrees[0].path, PathBuf::from("/repo"));
        assert_eq!(worktrees[0].branch.as_deref(), Some("main"));
        assert_eq!(worktrees[1].path, PathBuf::from("/repo-wt/feature-a"));
        assert_eq!(worktrees[1].branch.as_deref(), Some("feature/a"));
        assert_eq!(worktrees[2].path, PathBuf::from("/repo-wt/bugfix"));
        assert_eq!(worktrees[2].branch.as_deref(), Some("bugfix"));
    }

    #[test]
    fn parses_detached_worktree_without_branch() {
        let worktrees = parse_worktree_porcelain(
            "worktree /repo-detached\nHEAD abcdef0123456789abcdef0123456789abcdef01\ndetached\n",
        );

        assert_eq!(worktrees.len(), 1);
        assert_eq!(worktrees[0].path, PathBuf::from("/repo-detached"));
        assert_eq!(worktrees[0].branch, None);
    }

    #[test]
    fn parses_empty_input_as_empty_worktrees() {
        let worktrees = parse_worktree_porcelain("");

        assert!(worktrees.is_empty());
    }

    #[test]
    fn parses_trailing_whitespace_and_extra_blank_lines() {
        let worktrees = parse_worktree_porcelain(
            "\n\nworktree /repo\nHEAD abcdef0123456789abcdef0123456789abcdef01\nbranch refs/heads/main\n\n   \n\nworktree /repo-wt/dev\nHEAD abcdef0123456789abcdef0123456789abcdef02\nbranch refs/heads/dev\n\n\n",
        );

        assert_eq!(worktrees.len(), 2);
        assert_eq!(worktrees[0].path, PathBuf::from("/repo"));
        assert_eq!(worktrees[0].branch.as_deref(), Some("main"));
        assert_eq!(worktrees[1].path, PathBuf::from("/repo-wt/dev"));
        assert_eq!(worktrees[1].branch.as_deref(), Some("dev"));
    }

    #[test]
    fn sort_worktrees_puts_default_branch_first() {
        let mut worktrees = vec![
            Worktree {
                path: PathBuf::from("/repo/alpha"),
                branch: Some("alpha".to_string()),
            },
            Worktree {
                path: PathBuf::from("/repo/main"),
                branch: Some("main".to_string()),
            },
        ];

        sort_worktrees(&mut worktrees, Some("main"));

        assert_eq!(worktrees[0].branch.as_deref(), Some("main"));
        assert_eq!(worktrees[1].branch.as_deref(), Some("alpha"));
    }

    #[test]
    fn sort_worktrees_orders_non_default_branches_by_branch_name() {
        let mut worktrees = vec![
            Worktree {
                path: PathBuf::from("/a/y"),
                branch: Some("zeta".to_string()),
            },
            Worktree {
                path: PathBuf::from("/z/x"),
                branch: Some("alpha".to_string()),
            },
        ];

        sort_worktrees(&mut worktrees, Some("main"));

        assert_eq!(worktrees[0].branch.as_deref(), Some("alpha"));
        assert_eq!(worktrees[1].branch.as_deref(), Some("zeta"));
    }

    #[test]
    fn sort_worktrees_sinks_detached_entries_and_orders_them_by_path() {
        let mut worktrees = vec![
            Worktree {
                path: PathBuf::from("/repo/detached-z"),
                branch: None,
            },
            Worktree {
                path: PathBuf::from("/repo/feature"),
                branch: Some("feature".to_string()),
            },
            Worktree {
                path: PathBuf::from("/repo/detached-a"),
                branch: None,
            },
        ];

        sort_worktrees(&mut worktrees, Some("main"));

        assert_eq!(worktrees[0].branch.as_deref(), Some("feature"));
        assert_eq!(worktrees[1].branch, None);
        assert_eq!(worktrees[1].path, PathBuf::from("/repo/detached-a"));
        assert_eq!(worktrees[2].branch, None);
        assert_eq!(worktrees[2].path, PathBuf::from("/repo/detached-z"));
    }

    #[test]
    fn compute_branch_diff_short_circuits_default_branch() {
        let repo = repo_state(PathBuf::from("/definitely/not/a/git/repo"), Some("main"));

        let outcome = compute_branch_diff(&repo, "main").unwrap();

        assert_eq!(outcome, DiffOutcome::NoForwardChanges);
        // This test uses an invalid path; success proves the branch==default
        // guard returns before the Command::new("git") call.
    }

    #[test]
    fn compute_branch_diff_requires_default_branch() {
        let repo = repo_state(PathBuf::from("/definitely/not/a/git/repo"), None);

        let err = compute_branch_diff(&repo, "feature").unwrap_err();

        assert_eq!(err.stderr, "repo has no resolved default branch");
        assert!(err.command.contains("git -C "));
        assert!(err.command.contains("<default>...feature"));
    }

    #[test]
    fn compute_branch_diff_reports_git_failure() {
        let dir = tempfile::tempdir().unwrap();
        let repo = repo_state(dir.path().to_path_buf(), Some("main"));

        let err = compute_branch_diff(&repo, "feature").unwrap_err();

        assert!(err.command.contains("git -C "));
        assert!(err.command.contains("main...feature"));
        assert!(!err.stderr.is_empty());
    }

    #[test]
    fn truncate_diff_lines_reports_original_len_when_over_cap() {
        let stdout = (0..10_500)
            .map(|idx| format!("line {idx}"))
            .collect::<Vec<_>>()
            .join("\n");

        let (lines, truncated_at) = truncate_diff_lines(&stdout, 10_000);

        assert_eq!(lines.len(), 10_000);
        assert_eq!(truncated_at, Some(10_500));
        assert_eq!(lines[0], "line 0");
        assert_eq!(lines[9_999], "line 9999");
    }

    #[test]
    fn truncate_diff_lines_keeps_short_output_unmarked() {
        let stdout = "one\ntwo\nthree\nfour\nfive";

        let (lines, truncated_at) = truncate_diff_lines(stdout, 10_000);

        assert_eq!(lines, vec!["one", "two", "three", "four", "five"]);
        assert_eq!(truncated_at, None);
    }

    #[test]
    fn missing_repo_clones_successfully() {
        let (_remote, url) = init_bare_remote();
        let base = tempfile::tempdir().unwrap();
        let decl = repository("foo", url.clone());

        let state = ensure_repo(&decl, base.path()).unwrap();

        assert_eq!(state.id, "foo");
        assert_eq!(state.url, url);
        assert!(state.path.exists());
        assert!(state.worktrees.is_empty());
        assert!(state.last_worktree_scan.is_none());
        let git_dir = git_stdout(&state.path, &["rev-parse", "--git-dir"]);
        assert!(!git_dir.is_empty());
    }

    #[test]
    fn existing_ok_repo_is_left_clean() {
        let (_remote, url) = init_bare_remote();
        let base = tempfile::tempdir().unwrap();
        let decl = repository("foo", url);

        let state = ensure_repo(&decl, base.path()).unwrap();
        let before = git_stdout(&state.path, &["status", "--porcelain"]);
        let state = ensure_repo(&decl, base.path()).unwrap();
        let after = git_stdout(&state.path, &["status", "--porcelain"]);

        assert_eq!(before, "");
        assert_eq!(after, before);
        assert_eq!(state.path, base.path().join("foo"));
    }

    #[test]
    fn drifted_origin_refuses_without_changing_remote() {
        let (_remote, url) = init_bare_remote();
        let (_other_remote, other_url) = init_bare_remote();
        let base = tempfile::tempdir().unwrap();
        let decl = repository("foo", url.clone());
        let state = ensure_repo(&decl, base.path()).unwrap();
        let output = Command::new("git")
            .arg("-C")
            .arg(&state.path)
            .arg("remote")
            .arg("set-url")
            .arg("origin")
            .arg(&other_url)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git remote set-url failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let err = ensure_repo(&decl, base.path()).unwrap_err();

        match err {
            RepoStartupError::UrlDrift {
                id,
                declared,
                observed,
                path,
            } => {
                assert_eq!(id, "foo");
                assert_eq!(declared, url);
                assert_eq!(observed, other_url);
                assert_eq!(path, state.path);
            }
            other => panic!("expected UrlDrift, got {other:?}"),
        }
        assert_eq!(
            git_stdout(&state.path, &["remote", "get-url", "origin"]),
            other_url
        );
    }

    #[test]
    fn existing_non_git_path_refuses() {
        let (_remote, url) = init_bare_remote();
        let base = tempfile::tempdir().unwrap();
        fs::create_dir(base.path().join("x")).unwrap();
        let decl = repository("x", url);

        let err = ensure_repo(&decl, base.path()).unwrap_err();

        match err {
            RepoStartupError::NotAGitRepo { id, path } => {
                assert_eq!(id, "x");
                assert_eq!(path, base.path().join("x"));
            }
            other => panic!("expected NotAGitRepo, got {other:?}"),
        }
    }

    #[test]
    fn clone_failure_reports_stderr() {
        let base = tempfile::tempdir().unwrap();
        let decl = repository("foo", "file:///definitely-not-here-12345".to_string());

        let err = ensure_repo(&decl, base.path()).unwrap_err();

        match err {
            RepoStartupError::CloneFailed {
                id,
                url,
                path,
                stderr,
            } => {
                assert_eq!(id, "foo");
                assert_eq!(url, "file:///definitely-not-here-12345");
                assert_eq!(path, base.path().join("foo"));
                assert!(!stderr.is_empty());
            }
            other => panic!("expected CloneFailed, got {other:?}"),
        }
    }
}
