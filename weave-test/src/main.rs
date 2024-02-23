mod test_suite;
mod runner;
mod test_results;
mod ui;

use std::collections::HashMap;
use std::env::set_current_dir;
use std::fs;
use std::fs::metadata;
use std::process::exit;
use std::str::FromStr;
use std::time::Duration;
use regex::Regex;
use crate::runner::run;
use crate::test_suite::{load_from_folder, load_values_file, ValuesFile};
use crate::ui::{Ui, UIFormat};
use anyhow::Result;
use tokio::time::sleep;

#[tokio::main]
async fn main() -> Result<()>{
    let cmd = clap::Command::new("weave-test")
        .bin_name("weave-test")
        .arg(clap::arg!(--"values" <PATH> "Path to a settings file which will be passed to tests as environment variables."))
        .arg(clap::arg!(--"path" <PATH> "Path to where tests are running. Defaults to current directory."))
        .arg(clap::arg!(--"only" <PATTERN> "Only run tests in suites that match the regular expression."))
        .arg(clap::arg!(--"format" <FORMAT> "Format output to the terminal. Can be colour, plain, none or json. Defaults to colour"));
    let matches = cmd.get_matches();

    //Getting tests folder.
    let current_dir = ".".to_string();
    let path: &String = matches.get_one::<String>("path").unwrap_or(&current_dir);
    let path = fs::canonicalize(path).expect("Unable to resolve path passed in correctly!");
    let meta = metadata(path.clone()).expect("Was unable to find Path!");

    //Getting test suite name pattern
    let pattern = if let Some(pat) = matches.get_one::<String>("only"){
        Some(Regex::new(pat).expect("Invalid regular expression passed to only!"))
    } else{
        None
    };

    //Getting output format.
    let ui_format = "colour".to_string();
    let ui_format = matches.get_one::<String>("format").unwrap_or(&ui_format);
    let ui_format = UIFormat::from_str(ui_format.as_str())?;


    let dataFile : Option<ValuesFile> =if let Some(config_path) = matches.get_one::<String>("values") {
        let config_path = fs::canonicalize(config_path)?;

        if !config_path.exists() {
            eprintln!("Unable to find values file {config_path:?}!");
            exit(4);
        }

        Some(load_values_file(config_path.to_str().unwrap())?)
    }else{
        None
    };

    if !meta.is_dir() {
        eprintln!("Expected a folder to be passed in as the test path!");
        exit(5);
    }

    if !path.exists(){
        eprintln!("Unable to find path {path:?}!");
        exit(4);
    }

    let mut ui = Ui::new(ui_format.clone()).await;

    set_current_dir(path.clone()).expect("Was unable to set current directory!");

    let test_suites= load_from_folder(path.as_path().to_str().unwrap()).unwrap();

    let result = run(&test_suites, &pattern, &dataFile, &mut ui).await?;

    sleep(Duration::from_millis(100)).await;
    ui.close().await?;

    if(ui_format == UIFormat::Json){
        let json = serde_json::to_string_pretty(&result)?;
        println!("{json}");
    }

    Ok(())

}