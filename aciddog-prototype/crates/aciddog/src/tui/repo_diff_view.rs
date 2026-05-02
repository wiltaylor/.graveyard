//! State and pure line styling for the in-TUI repository diff view.

use std::path::Path;

use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap},
    Frame,
};

use crate::repos::DiffOutcome;

use super::app::{App, Focus};

#[derive(Debug, Clone)]
pub struct RepoDiffScreen {
    pub repo_idx: usize,
    pub worktree_idx: usize,
    pub outcome: RepoDiffOutcome,
    pub scroll: u16,
    pub last_visible_lines: u16,
    pub last_max_scroll: u16,
}

#[derive(Debug, Clone)]
pub enum RepoDiffOutcome {
    NoChanges {
        reason: &'static str,
    },
    Diff {
        lines: Vec<Line<'static>>,
        truncated_at: Option<usize>,
    },
    Error {
        command: String,
        stderr: String,
    },
}

impl RepoDiffScreen {
    pub fn new(
        repo_idx: usize,
        worktree_idx: usize,
        repo_path: &Path,
        default_branch: &str,
        branch: &str,
        outcome: DiffOutcome,
    ) -> Self {
        let outcome = match outcome {
            DiffOutcome::NoForwardChanges => RepoDiffOutcome::NoChanges {
                reason: if branch == default_branch {
                    "No diff (branch matches default)."
                } else {
                    "No forward changes."
                },
            },
            DiffOutcome::Diff {
                lines,
                truncated_at,
            } => {
                let mut styled_lines = lines.into_iter().map(style_diff_line).collect::<Vec<_>>();
                if let Some(original_len) = truncated_at {
                    let command = diff_command_string(repo_path, default_branch, branch);
                    styled_lines.push(Line::from(Span::styled(
                        format!(
                            "… diff exceeds 10 000 lines (got {original_len}); run `{command}` for the full diff."
                        ),
                        Style::default().fg(Color::Yellow),
                    )));
                }
                RepoDiffOutcome::Diff {
                    lines: styled_lines,
                    truncated_at,
                }
            }
        };

        Self {
            repo_idx,
            worktree_idx,
            outcome,
            scroll: 0,
            last_visible_lines: 0,
            last_max_scroll: 0,
        }
    }

    pub fn error(
        repo_idx: usize,
        worktree_idx: usize,
        command: impl Into<String>,
        stderr: impl Into<String>,
    ) -> Self {
        Self {
            repo_idx,
            worktree_idx,
            outcome: RepoDiffOutcome::Error {
                command: command.into(),
                stderr: stderr.into(),
            },
            scroll: 0,
            last_visible_lines: 0,
            last_max_scroll: 0,
        }
    }
}

fn style_diff_line(line: String) -> Line<'static> {
    let style = if line.starts_with('+') && !line.starts_with("+++") {
        Style::default().fg(Color::Green)
    } else if line.starts_with('-') && !line.starts_with("---") {
        Style::default().fg(Color::Red)
    } else if line.starts_with("@@") {
        Style::default().fg(Color::DarkGray)
    } else if line.starts_with("diff --git")
        || line.starts_with("index ")
        || line.starts_with("+++ ")
        || line.starts_with("--- ")
    {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    };

    if style == Style::default() {
        Line::from(line)
    } else {
        Line::from(Span::styled(line, style))
    }
}

fn diff_command_string(path: &Path, default_branch: &str, branch: &str) -> String {
    format!(
        "git -C {} diff --no-color {default_branch}...{branch}",
        path.display()
    )
}

pub fn render_repo_diff_screen(f: &mut Frame, area: Rect, app: &mut App) -> Rect {
    let focused = app.focus == Focus::RepoDiff;
    let Some(screen) = app.repo_diff_screen.as_mut() else {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" diff ")
            .border_style(if focused {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default().fg(Color::DarkGray)
            });
        let inner = block.inner(area);
        f.render_widget(block, area);
        app.layout.repo_diff_section = Some(inner);
        return inner;
    };

    let repo = app.repos.get(screen.repo_idx);
    let worktree = repo.and_then(|repo| repo.worktrees.get(screen.worktree_idx));
    let repo_id = repo.map(|repo| repo.id.as_str()).unwrap_or("<repo>");
    let default_branch = repo
        .and_then(|repo| repo.default_branch.as_deref())
        .unwrap_or("<default>");
    let branch = worktree
        .and_then(|worktree| worktree.branch.as_deref())
        .unwrap_or("<branch>");

    let title = match screen.outcome {
        RepoDiffOutcome::Error { .. } => " diff failed ".to_string(),
        RepoDiffOutcome::NoChanges { .. } => {
            format!(" no diff: {repo_id} · {branch} ← {default_branch} ")
        }
        _ => format!(" diff: {repo_id} · {branch} ← {default_branch} "),
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .title_bottom(" j/k scroll · PgUp/PgDn page · Home/End · Esc/q close ")
        .border_style(if focused {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        });
    let inner = block.inner(area);
    f.render_widget(block, area);

    let visible = inner.height;
    let total_lines = match &screen.outcome {
        RepoDiffOutcome::NoChanges { .. } => 1,
        RepoDiffOutcome::Diff {
            lines,
            truncated_at,
        } => {
            let _truncated = truncated_at.is_some();
            lines.len()
        }
        RepoDiffOutcome::Error { stderr, .. } => 2 + stderr.lines().count(),
    };
    screen.last_visible_lines = visible;
    screen.last_max_scroll =
        (total_lines.saturating_sub(visible as usize)).min(u16::MAX as usize) as u16;
    screen.scroll = screen.scroll.min(screen.last_max_scroll);
    app.layout.repo_diff_section = Some(inner);

    match &screen.outcome {
        RepoDiffOutcome::NoChanges { reason } => {
            let p = Paragraph::new(*reason)
                .style(Style::default().fg(Color::DarkGray))
                .alignment(Alignment::Center)
                .wrap(Wrap { trim: false });
            f.render_widget(p, inner);
        }
        RepoDiffOutcome::Diff { lines, .. } => {
            let p = Paragraph::new(lines.clone()).scroll((screen.scroll, 0));
            f.render_widget(p, inner);
        }
        RepoDiffOutcome::Error { command, stderr } => {
            let mut lines = vec![
                Line::from("command:"),
                Line::from(command.clone()),
                Line::from("stderr:"),
            ];
            lines.extend(stderr.lines().map(|line| Line::from(line.to_string())));
            if stderr.is_empty() {
                lines.push(Line::from(""));
            }
            let p = Paragraph::new(lines)
                .alignment(Alignment::Center)
                .scroll((screen.scroll, 0))
                .wrap(Wrap { trim: false });
            f.render_widget(p, inner);
        }
    }

    if screen.last_max_scroll > 0 {
        let mut sb_state = ScrollbarState::new(total_lines)
            .viewport_content_length(visible as usize)
            .position(screen.scroll as usize);
        let sb = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None);
        let sb_area = Rect {
            x: area.x + area.width.saturating_sub(1),
            y: inner.y,
            width: 1,
            height: inner.height,
        };
        f.render_stateful_widget(sb, sb_area, &mut sb_state);
    }

    inner
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use ratatui::{backend::TestBackend, buffer::Buffer, Terminal};

    use super::*;
    use crate::{
        config::{AciddogConfig, Team},
        repos::{RepoState, Worktree},
    };

    fn test_app(screen: RepoDiffScreen) -> App {
        let mut app = App::new(AciddogConfig {
            team: Team {
                id: "t".into(),
                description: "d".into(),
                replace_thinking_messages: false,
                default_ai_core: None,
                default_idle_timeout: None,
            },
            agents: vec![],
            workers: vec![],
            automatons: vec![],
            ai_cores: vec![],
            memory_banks: vec![],
            compactions: vec![],
            channels: vec![],
            repositories: vec![],
            workflows: vec![],
            webhooks: vec![],
            thinking_messages: vec!["thinking".into()],
        });
        app.repos.push(RepoState {
            id: "repo".into(),
            url: "git@example.com/repo.git".into(),
            path: PathBuf::from("/tmp/repo"),
            default_branch: Some("main".into()),
            worktrees: vec![Worktree {
                path: PathBuf::from("/tmp/repo"),
                branch: Some("feature".into()),
            }],
            last_worktree_scan: None,
        });
        app.active_view = super::super::app::ActiveView::RepoDiff {
            repo_idx: 0,
            worktree_idx: 0,
        };
        app.focus = Focus::RepoDiff;
        app.repo_diff_screen = Some(screen);
        app
    }

    fn render(app: &mut App) -> Buffer {
        let mut terminal = Terminal::new(TestBackend::new(100, 18)).unwrap();
        terminal
            .draw(|f| {
                render_repo_diff_screen(f, Rect::new(0, 0, 100, 18), app);
            })
            .unwrap();
        terminal.backend().buffer().clone()
    }

    fn buffer_text(buf: &Buffer) -> String {
        let mut out = String::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                out.push_str(buf[(x, y)].symbol());
            }
            out.push('\n');
        }
        out
    }

    fn first_fg(line: &Line<'static>) -> Option<Color> {
        line.spans.first().and_then(|span| span.style.fg)
    }

    #[test]
    fn diff_lines_are_styled_by_unified_diff_prefix() {
        let screen = RepoDiffScreen::new(
            0,
            0,
            PathBuf::from("/tmp/repo").as_path(),
            "main",
            "feature",
            DiffOutcome::Diff {
                lines: vec![
                    "diff --git a/a b/a".into(),
                    "index abc..def 100644".into(),
                    "--- a/a".into(),
                    "+++ b/a".into(),
                    "@@ -1 +1 @@".into(),
                    "-old".into(),
                    "+new".into(),
                    " context".into(),
                ],
                truncated_at: None,
            },
        );
        let mut app = test_app(screen);
        render(&mut app);

        let RepoDiffOutcome::Diff { lines, .. } = &app.repo_diff_screen.as_ref().unwrap().outcome
        else {
            panic!("expected diff outcome");
        };
        assert_eq!(first_fg(&lines[0]), Some(Color::Cyan));
        assert_eq!(first_fg(&lines[1]), Some(Color::Cyan));
        assert_eq!(first_fg(&lines[2]), Some(Color::Cyan));
        assert_eq!(first_fg(&lines[3]), Some(Color::Cyan));
        assert_eq!(first_fg(&lines[4]), Some(Color::DarkGray));
        assert_eq!(first_fg(&lines[5]), Some(Color::Red));
        assert_eq!(first_fg(&lines[6]), Some(Color::Green));
        assert_eq!(first_fg(&lines[7]), None);
    }

    #[test]
    fn no_changes_renders_centered_copy() {
        let screen = RepoDiffScreen::new(
            0,
            0,
            PathBuf::from("/tmp/repo").as_path(),
            "main",
            "feature",
            DiffOutcome::NoForwardChanges,
        );
        let mut app = test_app(screen);
        let buf = render(&mut app);
        let text = buffer_text(&buf);

        assert!(text.contains("No forward changes."));
        assert!(app.layout.repo_diff_section.is_some());
    }

    #[test]
    fn error_renders_title_command_and_stderr() {
        let screen = RepoDiffScreen::error(
            0,
            0,
            "git -C /tmp/repo diff --no-color main...feature",
            "fatal: bad revision",
        );
        let mut app = test_app(screen);
        let buf = render(&mut app);
        let text = buffer_text(&buf);

        assert!(text.contains("diff failed"));
        assert!(text.contains("git -C /tmp/repo diff --no-color main...feature"));
        assert!(text.contains("fatal: bad revision"));
    }

    #[test]
    fn truncated_diff_appends_yellow_trailer() {
        let screen = RepoDiffScreen::new(
            0,
            0,
            PathBuf::from("/tmp/repo").as_path(),
            "main",
            "feature",
            DiffOutcome::Diff {
                lines: vec!["+kept".into()],
                truncated_at: Some(10_001),
            },
        );
        let mut app = test_app(screen);
        render(&mut app);

        let RepoDiffOutcome::Diff { lines, .. } = &app.repo_diff_screen.as_ref().unwrap().outcome
        else {
            panic!("expected diff outcome");
        };
        let trailer = lines.last().unwrap();
        assert_eq!(first_fg(trailer), Some(Color::Yellow));
        assert!(trailer.to_string().contains("exceeds 10 000"));
    }
}
