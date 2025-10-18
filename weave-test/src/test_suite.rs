use std::collections::HashMap;
use std::fs;
use serde::{Deserialize, Serialize};
use anyhow::{bail, Result};
use glob::glob;

#[derive(Serialize, Deserialize, Debug)]
pub struct TestSuite {
    pub name: String,
    pub author: Option<String>,
    pub description: Option<String>,
    pub env: Option<HashMap<String, String>>,
    pub steps: Vec<TestStep>,
    pub data_sets: Option<HashMap<String, Vec<HashMap<String, String>>>>
}

#[derive(Serialize, Deserialize, Debug)]
pub struct TestStep {
    pub name: Option<String>,
    pub description: Option<String>,
    pub skip: Option<bool>,
    pub command: String,
    pub env: Option<HashMap<String, String>>,
    pub data_set: Option<String>,
    pub timeout: Option<u64>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ValuesFile {
    pub env: HashMap<String, String>,
    pub data_sets: Option<HashMap<String, Vec<HashMap<String, String>>>>
}

pub fn load_values_file(path: &str) -> Result<ValuesFile> {
    let txt = fs::read_to_string(path)?;
    let result = serde_yaml::from_str(&txt)?;
    Ok(result)
}

pub fn load_from_folder(path: &str) -> Result<Vec<TestSuite>> {
    let search_pattern = format!("{}/{}", path, "*_test.yaml" );
    let mut result: Vec<TestSuite> = vec![];

    for entry in glob(&search_pattern).expect("Invalid path passed to load tests!") {
        match entry{
            Ok(path) => {
                let text = fs::read_to_string(&path)?;
                let test = serde_yaml::from_str(text.as_str())?;
                result.push(test);
            },
            Err(e) => bail!(e),
        }
    }

    Ok(result)
}