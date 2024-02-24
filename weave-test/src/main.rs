use std::fs;
use std::fs::metadata;
use std::process::exit;
use std::str::FromStr;
use weave_test::ui::UIFormat;
use anyhow::Result;
use weave_test::TestSession;

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
    let pattern = matches.get_one::<String>("only").cloned();

    //Getting output format.
    let ui_format = "colour".to_string();
    let ui_format = matches.get_one::<String>("format").unwrap_or(&ui_format);
    let ui_format = UIFormat::from_str(ui_format.as_str())?;

    let data_file = matches.get_one::<String>("values").cloned();

    if !meta.is_dir() {
        eprintln!("Expected a folder to be passed in as the test path!");
        exit(5);
    }

    if !path.exists(){
        eprintln!("Unable to find path {path:?}!");
        exit(4);
    }

    let path = path.as_path().to_str().unwrap().to_string();

    let mut session = TestSession{
        ui_format,
        path,
        pattern,
        values_file: data_file,
    };

    session.run().await?;

    Ok(())

}