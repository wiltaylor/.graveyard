use std::fmt;
use std::fmt::Formatter;
use serde::{Deserialize, Serialize};

#[derive(PartialEq, Debug, Serialize, Deserialize, Clone)]
pub enum TestResult {
    Pass,
    Fail,
    NotRun,
    Inconclusive,
    Skip,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TestSuiteResult {
    pub name: String,
    pub overall_result: TestResult,
    pub steps: Vec<TestStepResult>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TestStepResult {
    pub name: Option<String>,
    pub result: TestResult,
    pub asserts: Vec<AssertResult>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AssertResult {
    pub message: String,
    pub success: bool,
    pub data_set_row: Option<usize>,
}

impl fmt::Display for TestResult {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self{
            TestResult::Pass => write!(f, "Pass"),
            TestResult::Fail => write!(f, "Fail"),
            TestResult::NotRun => write!(f, "NotRun"),
            TestResult::Inconclusive => write!(f, "Inconclusive"),
            TestResult::Skip => write!(f, "Skip"),
        }
    }
}