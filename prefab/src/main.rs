mod tui;
use anyhow::{anyhow, Ok, Result};
use prefab::config::load_config;
use tui::{run, setup_terminal};

use core::panic;

use std::path::Path;
use std::result::Result::Ok as Ok2;
use std::{env, path::PathBuf};

use clap::{arg, command, ArgAction};
use prefab::template::Template;

use crate::tui::restore_terminal;

fn list_templates(root: &Path) -> Result<Vec<Template>> {
    let mut result = vec![];

    if let Ok2(files) = root.read_dir() {
        for item in files.flatten() {
            let mut path = PathBuf::new();
            path.push(item.path());
            path.push("prefab.toml");

            if path.exists() {
                path.pop();
                if let Ok2(tmp) = Template::load(path) {
                    result.push(tmp);
                }
            }
        }
    }

    Ok(result)
}

fn get_template_directory() -> Result<PathBuf> {
    if let Ok2(path) = env::var("PREFAB_TEMPLATE_DIR") {
        let path = PathBuf::from(path);
        if !path.exists() {
            return Err(anyhow!(
                "Path sepcified by $PREFAB_TEMPLATE_DIR doesn't exist!"
            ));
        }

        return Ok(path);
    }

    if cfg!(target_os = "windows") {
        let mut path = PathBuf::new();

        let userprofile = env::var("USERPROFILE")?;
        path.push(userprofile);
        path.push(".prefab/templates");

        if path.exists() {
            return Ok(path);
        }
    } else {
        let mut path = PathBuf::new();

        let userprofile = env::var("HOME")?;
        path.push(&userprofile);
        path.push(".config/prefab/templates");

        if path.exists() {
            return Ok(path);
        }

        path.clear();
        path.push(&userprofile);
        path.push(".prefab/templates");

        if path.exists() {
            return Ok(path);
        }
    };

    Err(anyhow!("Can't find a template directory!"))
}

fn main() {
    let matches = command!()
        .arg(arg!([name] "Template to use"))
        .arg(arg!(-q --quiet "Don't show tui just create template").action(ArgAction::SetTrue))
        .arg(
            arg!(-v --var <variable> "Sets a variable (use \"name=value\" format)")
                .action(ArgAction::Append),
        )
        .arg(arg!(-c --config <FILE> "Sets a custom config file"))
        .arg(arg!(-l --list "Lists all of the templates installed on your system").group("list_group"))
        .arg(arg!(-d --dir <FOLDER> "Set the folder to create the result of the template in. Defaults to current directory."))
        .get_matches();

    let template_directory = if let Ok2(dir) = get_template_directory() {
        dir
    } else {
        panic!("Unable to find templat root director!")
    };

    if matches.get_flag("list") {
        println!("-- Template List --");
        let tmps = list_templates(&template_directory).unwrap();

        for i in tmps {
            println!(
                "[{}]- {}\n{}\n",
                i.source_path.file_name().unwrap().to_str().unwrap(),
                i.manifest.title.unwrap(),
                i.manifest.description.unwrap()
            )
        }

        return;
    }

    if let Some(name) = matches.get_one::<String>("name") {
        let mut buf = PathBuf::new();

        buf.push(template_directory);
        buf.push(name);

        if let Ok2(tmp) = Template::load(buf) {
            let mut tmp = tmp;
            //Get config

            let mut options = tmp.get_options();
            
            if let Some(cfg) = matches.get_one::<String>("config") {
                options = load_config( cfg, options).unwrap();
            }

            if let Some(vars) = matches.get_many::<String>("var") {
                for v in vars {
                    let parts: Vec<String> = v.split('=').map(|v| v.to_string()).collect();

                    let o = options.get(&parts[0]).unwrap().clone();
                    options.remove(&parts[0]);
                    let o = o.set_value(parts[1].clone());
                    options.insert(parts[0].clone(), o);
                }
            }

            if !matches.get_flag("quiet") {
                let mut terminal = setup_terminal().unwrap();
                run(&mut terminal, &mut options).unwrap();
                restore_terminal(&mut terminal).unwrap();
            }

            tmp.set_options(options);

            let resolve_path = if let Some(dir) = matches.get_one::<String>("dir") { dir.clone() } else { env::current_dir().unwrap().to_str().unwrap().to_string() };
            tmp.apply(resolve_path).unwrap();
        }
    } else {
        println!("Expected you to pass the name of a template in.");
    }
}
