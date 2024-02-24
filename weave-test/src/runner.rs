use std::collections::HashMap;
use std::time::Duration;
use regex::Regex;
use crate::test_results::{AssertResult, TestResult, TestStepResult, TestSuiteResult};
use crate::test_suite::{TestStep, TestSuite, ValuesFile};
use crate::ui::Ui;
use anyhow::Result;
use crate::environment::HashMapExt;
use crate::exec::RunningCommand;

struct TestSuiteRunner<'a> {
    suite: &'a TestSuite,
    ui: &'a mut Ui,
    value_file: &'a Option<ValuesFile>,
    data_sets: HashMap<String, Vec<HashMap<String, String>>>,
}

impl TestSuiteRunner<'_> {
    fn new<'a>(suite: &'a TestSuite, ui: &'a mut Ui, value_file: &'a Option<ValuesFile>) -> TestSuiteRunner<'a> {
        TestSuiteRunner{
            suite,
            ui,
            value_file,
            data_sets: HashMap::new(),
        }
    }

    async fn run(&mut self) -> Result<TestSuiteResult>{
        let mut result = TestSuiteResult{
            name: self.suite.name.clone(),
            overall_result: TestResult::Pass,
            steps: vec![],
        };

        self.data_sets =  if let Some(value) = self.value_file {
            value.data_sets.clone().unwrap_or(HashMap::new())
        }else{
            HashMap::new()
        };

        if let Some(value) = &self.suite.data_sets {
            for (k, v) in value {
                if self.data_sets.contains_key(k) {
                    self.data_sets.remove(k);
                }

                self.data_sets.insert(k.clone(), v.clone());
            }
        }

        if let Some(v) = self.value_file {
            if let Some(d) = &v.data_sets {
                for i in d.keys(){
                    if self.data_sets.contains_key(i){
                        self.data_sets.remove(i);
                    }

                    self.data_sets.insert(i.clone(), d[i].clone());
                }
            }
        }


        self.ui.start_suite(&self.suite.name).await?;

        let mut failed = false;
        for step in &self.suite.steps {

            //Setting status of all remaining tests as not run.
            if failed {
                result.steps.push(TestStepResult{
                    name: step.name.clone(),
                    result: TestResult::NotRun,
                    asserts: vec![],
                });
                continue;
            }

            if let Some(v) = &step.skip  {
                if *v {
                    result.steps.push(TestStepResult{
                        name: step.name.clone(),
                        result: TestResult::Skip,
                        asserts: vec![],
                    });
                }
                continue;
            }

            //let step_result = run_step(step, &self.suite.env, &value_env, &data_sets, self.ui).await?;
            let step_result = self.run_step(step).await?;

            if step_result.result == TestResult::Fail {
                failed = true;
                result.overall_result = TestResult::Fail;
            }

            if step_result.result == TestResult::Inconclusive {
                result.overall_result = TestResult::Inconclusive;
            }

            result.steps.push(step_result);
        }

        let _ = self.ui.finish_suite(&result.name, result.overall_result.clone()).await;

        Ok(result)
    }

    async fn run_step(&mut self, step: &TestStep) -> Result<TestStepResult> {

        let mut result = TestStepResult{
            name: step.name.clone(),
            result: TestResult::Inconclusive,
            asserts: vec![],
        };

        let mut env: HashMap<String, String> = HashMap::new();
        let time_out = if let Some(time) = step.timeout {
            time
        }else{
            300
        };

        env.try_append(&self.suite.env);
        env.try_append(&step.env);
        if let Some(value) = self.value_file {
            env.append(&value.env);
        }

        let name = step.name.clone().unwrap_or("".to_string());

        self.ui.start_step(&name).await?;

        if let Some(set_name)  = &step.data_set {
            let data_set = self.data_sets.get(set_name).unwrap();
            self.ui.start_set(set_name).await?;

            result.result = TestResult::NotRun;

            for (idx, row) in data_set.iter().enumerate() {
                self.ui.report_set_row(idx).await?;

                let mut set_env = env.clone();
                set_env.append(row);
                // append_environment(&mut set_env, &Some(row.clone()));


                let run_result = if let Ok(r) = execute_command(&step.command, set_env, self.ui, &mut result.asserts, Some(idx), time_out).await {
                    r
                }else{
                    self.ui.assert("Test Timeout Hit", false).await?;
                    result.asserts.push(AssertResult{
                        message: "Test timed out!".to_string(),
                        success: false,
                        data_set_row: Some(idx),
                    });
                    TestResult::Fail
                };

                if run_result == TestResult::Pass && result.result != TestResult::Fail && result.result != TestResult::Inconclusive {
                    result.result = TestResult::Pass;
                }

                if run_result == TestResult::Inconclusive && result.result != TestResult::Fail {
                    result.result = TestResult::Inconclusive;
                }

                if run_result == TestResult::Fail {
                    result.result = TestResult::Fail;
                }
            }

            self.ui.finish_set().await?;

        } else{
            result.result = execute_command(&step.command, env, self.ui, &mut result.asserts, None, time_out).await?;
        }

        self.ui.finish_step(&name, result.result.clone()).await?;

        Ok(result)
    }
}

pub async fn run(test_suites: &[TestSuite], pattern: &Option<Regex>, value_file: &Option<ValuesFile>, ui: &mut Ui) -> Result<Vec<TestSuiteResult>> {
    let mut result: Vec<TestSuiteResult> = vec![];

    for suite in test_suites.iter() {
        if let Some(pat) = pattern{
            if !pat.is_match(&suite.name){
                continue;
            }
        }
        let mut runner = TestSuiteRunner::new(suite, ui, value_file);
        result.push(runner.run().await?)

    }

    Ok(result)
}


async fn execute_command(command: &String, environment: HashMap<String, String>, ui: &mut Ui, asserts: &mut Vec<AssertResult>, row: Option<usize>, timeout: u64) -> Result<TestResult> {
    let mut exec = RunningCommand::new(command, &environment, Some(Duration::from_secs(timeout)))?;
    let mut result = TestResult::Inconclusive;

    'check_lines: loop {
        let line: Result<Option<String>> = exec.next_line().await;

        match line {
            Ok(val) => {
                if let Some(line) = val {
                    if let Some(txt) = line.strip_prefix("WEAVE-TEST:PRINT:") {
                        ui.print(txt).await?;
                    }
                    if let Some(txt) = line.strip_prefix("WEAVE-TEST:FAIL:") {
                        result = TestResult::Fail;

                        ui.assert(txt, false).await?;

                        asserts.push(AssertResult {
                            message:txt.to_string(),
                            success: false,
                            data_set_row: row,
                        });
                    }

                    if let Some(txt) = line.strip_prefix("WEAVE-TEST:PASS:") {
                        if result != TestResult::Fail {
                            result = TestResult::Pass;
                        }


                        ui.assert(txt, true).await?;

                        asserts.push(AssertResult {
                            message: txt.to_string(),
                            success: true,
                            data_set_row: row,
                        });
                    }
                } else {
                    break 'check_lines
                }
            }
            Err(_) => {
                result = TestResult::Fail;
                ui.assert("Test Timed Out!", false).await?;

                asserts.push(AssertResult {
                    message: "Test Timed Out!".to_string(),
                    success: false,
                    data_set_row: row,
                });

                break 'check_lines;
            }
        };
    };

    Ok(result.clone())

}