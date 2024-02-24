use std::env::set_current_dir;
use crate::ui::{Ui, UIFormat};
use anyhow::Result;
use regex::Regex;
use crate::runner::run;
use crate::test_suite::{load_from_folder, load_values_file};

mod test_suite;
mod runner;
mod test_results;
pub mod ui;
mod exec;
mod environment;

pub struct TestSession {
    pub ui_format: UIFormat,
    pub path: String,
    pub pattern: Option<String>,
    pub values_file: Option<String>,
}

impl TestSession {
    pub async fn run(&mut self) -> Result<()> {
        let mut ui = Ui::new(self.ui_format.clone()).await;

        let values_file = if let Some(val) = &self.values_file {
            Some(load_values_file(val)?)
        }else{
            None
        };

        let pattern = if let Some(val) = &self.pattern {
            Some(Regex::new(val)?)
        }else{
            None
        };

        set_current_dir(&self.path)?;
        let test_suites = load_from_folder(&self.path)?;
        let result = run(&test_suites, &pattern, &values_file, &mut ui).await?;

        ui.close().await?;

        if self.ui_format == UIFormat::Json {
            let json = serde_json::to_string_pretty(&result)?;
            println!("{json}");
        }

        Ok(())
    }
}