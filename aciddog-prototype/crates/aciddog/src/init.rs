use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{anyhow, Context, Result};
use include_dir::{include_dir, Dir, DirEntry};

static TEMPLATE: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../../init-team");
static ACIDDOG_SKILL: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../../.claude/skills/aciddog");

pub async fn run_init(target: PathBuf, force: bool) -> Result<ExitCode> {
    std::fs::create_dir_all(&target)
        .with_context(|| format!("creating target dir {}", target.display()))?;

    let wcl_dest = target.join("aciddog.wcl");
    if wcl_dest.exists() && !force {
        return Err(anyhow!(
            "{} already exists — pass --force to overwrite",
            wcl_dest.display()
        ));
    }

    extract_dir(&TEMPLATE, &target, force)?;
    let skill_dest = target.join(".aciddog").join("skills").join("aciddog");
    extract_dir(&ACIDDOG_SKILL, &skill_dest, force)?;

    println!("Initialised aciddog team in {}", target.display());
    println!("Next: cd {} && aciddog", target.display());
    Ok(ExitCode::SUCCESS)
}

fn extract_dir(src: &Dir<'_>, dest: &Path, force: bool) -> Result<()> {
    std::fs::create_dir_all(dest).with_context(|| format!("creating {}", dest.display()))?;
    for entry in src.entries() {
        let name = entry.path().file_name().ok_or_else(|| {
            anyhow!(
                "embedded entry has no file name: {}",
                entry.path().display()
            )
        })?;
        let dest_path = dest.join(name);
        match entry {
            DirEntry::Dir(d) => extract_dir(d, &dest_path, force)?,
            DirEntry::File(f) => {
                if dest_path.exists() && !force {
                    continue;
                }
                std::fs::write(&dest_path, f.contents())
                    .with_context(|| format!("writing {}", dest_path.display()))?;
            }
        }
    }
    Ok(())
}
