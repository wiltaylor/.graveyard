use std::collections::HashMap;
use std::process::Stdio;
use std::time::{Duration, SystemTime};
use regex::Regex;
use crate::test_results::{AssertResult, TestResult, TestStepResult, TestSuiteResult};
use crate::test_suite::{TestStep, TestSuite, ValuesFile};
use crate::ui::{Ui, UIFormat};
use anyhow::Result;
use tokio::process::{Command};
use tokio::time::timeout;
use tokio_stream::{Stream, StreamExt};
use tokio_util::codec::{FramedRead, LinesCodec};

pub async fn run(test_suites: &Vec<TestSuite>, pattern: &Option<Regex>, value_file: &Option<ValuesFile>, ui: &mut Ui) -> Result<Vec<TestSuiteResult>> {
    let mut result: Vec<TestSuiteResult> = vec![];

    for suite in test_suites.iter() {
        if let Some(pat) = pattern{
            if !pat.is_match(&suite.name){
                continue;
            }
        }

        result.push(run_suite(suite, value_file, ui).await?);

    }

    return Ok(result);
}

async fn run_suite(suite: &TestSuite, value_file: &Option<ValuesFile>, ui: &mut Ui) -> Result<TestSuiteResult> {
    let mut result = TestSuiteResult{
        name: suite.name.clone(),
        overall_result: TestResult::Pass,
        steps: vec![],
    };

    let mut data_sets =  if let Some(value) = value_file {
      value.data_sets.clone().unwrap_or(HashMap::new())
    }else{
        HashMap::new()
    };

    if let Some(value) = &suite.data_sets {
        for (k, v) in value {
            if data_sets.contains_key(k) {
                data_sets.remove(k);
            }

            data_sets.insert(k.clone(), v.clone());
        }
    }

    let value_env = if let Some(value) = value_file {
       Some(value.env.clone())
    }else{
      None
    };

    if let Some(v) = value_file {
        if let Some(d) = &v.data_sets {
            for i in d.keys(){
               if data_sets.contains_key(i){
                   data_sets.remove(i);
               }

               data_sets.insert(i.clone(), d[i].clone());
            }
        }
    }


    ui.start_suite(&suite.name).await?;

    let mut failed = false;
    for step in &suite.steps {

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

        let step_result = run_step(step, &suite.env, &value_env, &data_sets, ui).await?;

        if step_result.result == TestResult::Fail {
            failed = true;
            result.overall_result = TestResult::Fail;
        }

        if step_result.result == TestResult::Inconclusive {
            result.overall_result = TestResult::Inconclusive;
        }

        result.steps.push(step_result);
    }

    let _ = ui.finish_suite(&result.name, result.overall_result.clone()).await;

    Ok(result)
}

async fn run_step(step: &TestStep, suite_environment: &Option<HashMap<String, String>>, value_env: &Option<HashMap<String, String>>, data_sets: &HashMap<String, Vec<HashMap<String, String>>>, ui: &mut Ui) -> Result<TestStepResult> {
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

    append_environment(&mut env, suite_environment);
    append_environment(&mut env, &step.env);
    append_environment(&mut env, &value_env);

    let name = step.name.clone().unwrap_or("".to_string());

    ui.start_step(&name).await?;

    if let Some(set_name)  = &step.data_set {
        let data_set = data_sets.get(set_name).unwrap();
        ui.start_set(set_name).await?;

        result.result = TestResult::NotRun;

        for (idx, row) in data_set.iter().enumerate() {
            ui.report_set_row(idx).await?;

            let mut set_env = env.clone();
            append_environment(&mut set_env, &Some(row.clone()));


            let run_result = if let Ok(r) = timeout(Duration::from_secs(time_out),  execute_command(&step.command, set_env, ui, &mut result.asserts, Some(idx))).await {
                r?
            }else{
                ui.assert("Test Timeout Hit", false).await?;
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

        ui.finish_set().await?;

    } else{
       result.result = execute_command(&step.command, env, ui, &mut result.asserts, None).await?;
    }

    ui.finish_step(&name, result.result.clone()).await?;

    Ok(result)
}

fn append_environment(target: &mut HashMap<String, String>, right: &Option<HashMap<String, String>> ) {
    if let Some(right) = right {
        for (k, v) in right.iter() {
            if target.contains_key(k) {
                target.remove(k);
            }

            target.insert(k.clone(), v.clone());
        }
    }
}

async fn execute_command(command: &String, environment: HashMap<String, String>, ui: &mut Ui, asserts: &mut Vec<AssertResult>, row: Option<usize>) -> Result<TestResult> {
    let mut proc = if cfg!(target_os = "windows") {
        Command::new("cmd")
            .envs(environment)
            .args(["/C", command])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .stdin(Stdio::null())
            .spawn()?
    } else {
        Command::new("sh")
            .envs(environment)
            .args(["-c", command])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::null())
            .spawn()?
    };

    let stdout = FramedRead::new(proc.stdout.take().unwrap(), LinesCodec::new())
        .map(|data| data.expect("fail on out!"));

    let stderr = FramedRead::new(proc.stderr.take().unwrap(), LinesCodec::new())
        .map(|data| data.expect("fail on err!"));

    let mut stream = stdout.chain(stderr);
    let mut result = TestResult::Inconclusive;

    while let Some(line) = stream.next().await {

        if line.starts_with("WEAVE-TEST:PRINT:") {
            ui.print(&line[17..].trim()).await?;
        }

        if line.starts_with("WEAVE-TEST:FAIL:"){
            result = TestResult::Fail;
            let text = line[16..].trim().to_string();

            ui.assert(&text, false).await?;

            asserts.push(AssertResult{
                message: text,
                success: false,
                data_set_row: row,
            });

        }

        if line.starts_with("WEAVE-TEST:PASS:") {
            if result != TestResult::Fail {
                result = TestResult::Pass;
            }

            let text = line[16..].trim().to_string();

            ui.assert(&text,true).await?;

            asserts.push(AssertResult{
                message: text,
                success: true,
                data_set_row: row,
            });
        }
    }

    Ok(result)

}