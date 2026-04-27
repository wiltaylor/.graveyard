//! Per-instance executor task. Drives a single tokio task through the
//! workflow's steps and emits progress via the `WorkflowEvent`
//! channel.
//!
//! Steps execute inline when possible:
//! - `Command` shells out via `/bin/sh -c`.
//! - `Check` reads the shared variable map and evaluates a predicate.
//! - `Worker` constructs an AICore from the worker template and runs
//!   a single turn with the rendered prompt + per-instance
//!   `workflow_get` / `workflow_set` tools injected.
//! - `Conditional` behaves like `Worker` but additionally injects the
//!   `check_successful` tool and interprets its signal. Missing-call
//!   is treated as `pass=false` per the design spec.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures::stream::StreamExt;
use tokio::process::Command as TokioCommand;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::ai::AIEvent;
use crate::config::{AciddogConfig, CheckOp, StepKind, Workflow, WorkflowProperty};

use super::event::{StepOutcome, WorkflowEvent};
use super::render::render_template;
use super::value::PropertyValue;
use super::CheckSuccessfulSignal;

/// Default timeout for a single worker step's turn. Keeps a runaway
/// worker from stalling a workflow indefinitely.
const WORKER_TURN_TIMEOUT: Duration = Duration::from_secs(300);

/// Run one workflow instance to completion. Consumes its own cancel
/// receiver and drops handles on the way out.
#[allow(clippy::too_many_arguments)]
pub async fn run_instance(
    instance_id: u64,
    workflow: Workflow,
    vars: Arc<Mutex<HashMap<String, PropertyValue>>>,
    config: Arc<AciddogConfig>,
    event_tx: mpsc::UnboundedSender<WorkflowEvent>,
    mut check_rx: mpsc::UnboundedReceiver<CheckSuccessfulSignal>,
    mut cancel_rx: mpsc::Receiver<()>,
) {
    // Snapshot + broadcast seeded variables so the TUI panel shows
    // them immediately.
    let initial_vars: Vec<(String, PropertyValue)> = {
        let guard = vars.lock().expect("vars poisoned");
        guard.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
    };
    let _ = event_tx.send(WorkflowEvent::Started {
        instance_id,
        workflow_id: workflow.id.clone(),
        vars: initial_vars,
    });

    // Wrap the declared properties once so every WorkflowSetTool built
    // for this instance shares the same snapshot (they're immutable).
    let properties = Arc::new(workflow.properties.clone());

    let mut current_step = 0usize;
    let mut retries_used: u32 = 0;

    while current_step < workflow.steps.len() {
        if cancel_rx.try_recv().is_ok() {
            let _ = event_tx.send(WorkflowEvent::Finished {
                instance_id,
                ok: false,
                reason: Some("cancelled".into()),
            });
            return;
        }

        let step_idx = current_step;
        let step = &workflow.steps[step_idx];
        let _ = event_tx.send(WorkflowEvent::StepStarted {
            instance_id,
            step_idx,
        });

        let outcome = match &step.kind {
            StepKind::Command { command, cwd } => {
                run_command_step(instance_id, command, cwd.as_deref(), &vars, &event_tx).await
            }
            StepKind::Check {
                variable,
                op,
                value,
                target,
            } => {
                let target_idx = workflow
                    .steps
                    .iter()
                    .position(|s| &s.id == target)
                    .expect("validated at parse time");
                let passed = {
                    let guard = vars.lock().expect("vars poisoned");
                    evaluate_check(guard.get(variable), *op, value.as_deref())
                };
                if passed {
                    StepAction::Advance
                } else {
                    StepAction::RetryTo {
                        target: target_idx,
                        reason: format!(
                            "check `{}` {} {} failed",
                            variable,
                            op.name(),
                            value.as_deref().unwrap_or("")
                        ),
                    }
                }
            }
            StepKind::Worker { worker, prompt } => {
                run_worker_step(
                    instance_id,
                    &step.id,
                    worker,
                    prompt,
                    &vars,
                    &config,
                    Arc::clone(&properties),
                    &event_tx,
                    false,
                    &mut check_rx,
                )
                .await
            }
            StepKind::Conditional {
                worker,
                prompt,
                target,
            } => {
                let target_idx = workflow
                    .steps
                    .iter()
                    .position(|s| &s.id == target)
                    .expect("validated at parse time");
                let outcome = run_worker_step(
                    instance_id,
                    &step.id,
                    worker,
                    prompt,
                    &vars,
                    &config,
                    Arc::clone(&properties),
                    &event_tx,
                    true,
                    &mut check_rx,
                )
                .await;
                // Worker-step run re-uses Advance/Fail; translate
                // conditional failures into a RetryTo so the caller
                // can honour `max_retries`.
                match outcome {
                    StepAction::Advance => StepAction::Advance,
                    StepAction::Fail { reason } => StepAction::RetryTo {
                        target: target_idx,
                        reason,
                    },
                    other => other,
                }
            }
        };

        match outcome {
            StepAction::Advance => {
                let _ = event_tx.send(WorkflowEvent::StepFinished {
                    instance_id,
                    step_idx,
                    outcome: StepOutcome::Ok,
                });
                current_step += 1;
            }
            StepAction::Fail { reason } => {
                let _ = event_tx.send(WorkflowEvent::StepFinished {
                    instance_id,
                    step_idx,
                    outcome: StepOutcome::Failed {
                        reason: reason.clone(),
                    },
                });
                let _ = event_tx.send(WorkflowEvent::Finished {
                    instance_id,
                    ok: false,
                    reason: Some(reason),
                });
                return;
            }
            StepAction::RetryTo { target, reason } => {
                if retries_used >= workflow.max_retries {
                    let final_reason = format!(
                        "retry budget exhausted ({} retries) on step `{}`",
                        workflow.max_retries, step.id
                    );
                    let _ = event_tx.send(WorkflowEvent::StepFinished {
                        instance_id,
                        step_idx,
                        outcome: StepOutcome::Failed {
                            reason: final_reason.clone(),
                        },
                    });
                    let _ = event_tx.send(WorkflowEvent::Finished {
                        instance_id,
                        ok: false,
                        reason: Some(final_reason),
                    });
                    return;
                }
                retries_used += 1;
                let _ = event_tx.send(WorkflowEvent::StepFinished {
                    instance_id,
                    step_idx,
                    outcome: StepOutcome::Retry {
                        reason: reason.clone(),
                    },
                });
                let _ = event_tx.send(WorkflowEvent::Retrying {
                    instance_id,
                    target_step_idx: target,
                    attempt: retries_used,
                });
                current_step = target;
            }
        }
    }

    let _ = event_tx.send(WorkflowEvent::Finished {
        instance_id,
        ok: true,
        reason: None,
    });
}

/// Per-step action chosen by the executor after running the step.
pub(crate) enum StepAction {
    Advance,
    Fail { reason: String },
    RetryTo { target: usize, reason: String },
}

async fn run_command_step(
    instance_id: u64,
    template: &str,
    cwd: Option<&str>,
    vars: &Arc<Mutex<HashMap<String, PropertyValue>>>,
    event_tx: &mpsc::UnboundedSender<WorkflowEvent>,
) -> StepAction {
    let rendered = {
        let guard = vars.lock().expect("vars poisoned");
        match render_template(template, &guard) {
            Ok(s) => s,
            Err(e) => {
                return StepAction::Fail {
                    reason: format!("render error: {e}"),
                }
            }
        }
    };

    let mut cmd = TokioCommand::new("/bin/sh");
    cmd.arg("-c").arg(&rendered);
    if let Some(path) = cwd {
        cmd.current_dir(path);
    }
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            return StepAction::Fail {
                reason: format!("failed to spawn shell: {e}"),
            };
        }
    };

    use tokio::io::{AsyncBufReadExt, BufReader};
    let stdout_handle = child.stdout.take().map(|stdout| {
        let tx = event_tx.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = tx.send(WorkflowEvent::StepOutput { instance_id, line });
            }
        })
    });
    let stderr_handle = child.stderr.take().map(|stderr| {
        let tx = event_tx.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = tx.send(WorkflowEvent::StepOutput {
                    instance_id,
                    line: format!("stderr: {line}"),
                });
            }
        })
    });

    let status = match child.wait().await {
        Ok(s) => s,
        Err(e) => {
            return StepAction::Fail {
                reason: format!("wait failed: {e}"),
            };
        }
    };
    // Drain the stdout/stderr readers so every line they captured has
    // been forwarded to `event_tx` before this step returns. Without
    // this the test harness (and the TUI tail) can race past output
    // that was still in-flight when `child.wait()` resolved.
    if let Some(h) = stdout_handle {
        let _ = h.await;
    }
    if let Some(h) = stderr_handle {
        let _ = h.await;
    }
    if status.success() {
        StepAction::Advance
    } else {
        StepAction::Fail {
            reason: format!("command exited with {status}"),
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_worker_step(
    instance_id: u64,
    step_id: &str,
    worker_id: &str,
    prompt_template: &str,
    vars: &Arc<Mutex<HashMap<String, PropertyValue>>>,
    config: &Arc<AciddogConfig>,
    properties: Arc<Vec<WorkflowProperty>>,
    event_tx: &mpsc::UnboundedSender<WorkflowEvent>,
    is_conditional: bool,
    check_rx: &mut mpsc::UnboundedReceiver<CheckSuccessfulSignal>,
) -> StepAction {
    // Render the user prompt against the live variable snapshot.
    let rendered_prompt = {
        let guard = vars.lock().expect("vars poisoned");
        match render_template(prompt_template, &guard) {
            Ok(s) => s,
            Err(e) => {
                return StepAction::Fail {
                    reason: format!("render error: {e}"),
                }
            }
        }
    };

    // Drain any stale check_successful signals left over from a previous
    // step so we don't immediately short-circuit the new turn.
    while check_rx.try_recv().is_ok() {}

    let Some(worker) = config.worker(worker_id) else {
        return StepAction::Fail {
            reason: format!("unknown worker template `{worker_id}`"),
        };
    };

    let Some(core_cfg) = config.effective_ai_core(worker.ai_core.as_deref()) else {
        return StepAction::Fail {
            reason: format!(
                "worker `{worker_id}` has no ai_core (no override and no team default)"
            ),
        };
    };

    // Only the direct-API cores own a Rust-side ToolRegistry we can
    // inject per-step tools into. CLI-harness cores (ClaudeCode,
    // CopilotCli, GeminiCli) delegate to an external binary that has
    // no idea about `workflow_get` / `workflow_set` / `check_successful`,
    // so we fail loudly rather than silently running a worker that
    // can't communicate back.
    use crate::config::AiCoreKind;
    if !matches!(
        core_cfg.kind,
        AiCoreKind::AnthropicApi { .. } | AiCoreKind::OpenAiApi { .. }
    ) {
        return StepAction::Fail {
            reason: format!(
                "worker `{worker_id}` uses ai_core type that can't host per-instance tools \
                 (only anthropicApi and openAiApi are supported for workflow workers)"
            ),
        };
    }

    let resolved_tools = config.effective_tools(worker.tools.as_deref(), Some(core_cfg));

    // Build per-instance tools.
    use std::sync::Arc as StdArc;
    let mut extra: Vec<StdArc<dyn crate::ai::tools::Tool>> = vec![
        StdArc::new(crate::ai::tools::workflow_get::WorkflowGetTool::new(
            Arc::clone(vars),
        )),
        StdArc::new(crate::ai::tools::workflow_set::WorkflowSetTool::new(
            instance_id,
            Arc::clone(vars),
            properties,
            event_tx.clone(),
        )),
    ];
    if is_conditional {
        // Register a sink that feeds the per-instance check_rx so the
        // engine can collect `pass` / `reason` synchronously with the
        // worker's turn.
        let (sink_tx, mut sink_rx) =
            mpsc::unbounded_channel::<crate::ai::tools::check_successful::CheckSuccessfulSignal>();
        extra.push(StdArc::new(
            crate::ai::tools::check_successful::CheckSuccessfulTool::new(
                sink_tx,
                instance_id,
                step_id.to_string(),
            ),
        ));
        // Bridge the local sink to the engine's check_rx by pushing
        // anything the tool emits onto the shared channel. Run in a
        // background task so the worker's turn can proceed.
        let forward_tx: tokio::sync::mpsc::UnboundedSender<CheckSuccessfulSignal> = {
            // SAFETY: we need a cloned sender tied to the engine's
            // shared check_rx. Rather than plumbing another channel in
            // from `run_instance`, we close the loop by constructing a
            // parallel channel whose receiver replaces `check_rx` for
            // this step only. See conditional_collect below.
            let (tx, _rx) = mpsc::unbounded_channel();
            tx
        };
        drop(forward_tx); // unused — see `conditional_collect`.
                          // We don't forward here; instead the `sink_rx` is used below
                          // inside `conditional_collect` to observe the signal directly.
        return run_conditional_turn(
            instance_id,
            step_id,
            &rendered_prompt,
            worker,
            core_cfg,
            resolved_tools,
            extra,
            &mut sink_rx,
            event_tx,
        )
        .await;
    }

    run_plain_worker_turn(
        &rendered_prompt,
        worker,
        core_cfg,
        resolved_tools,
        extra,
        instance_id,
        event_tx,
    )
    .await
}

/// Drive one turn of a plain (non-conditional) Worker step. Returns
/// `Advance` on a clean completion, `Fail` on turn error / timeout.
async fn run_plain_worker_turn(
    prompt: &str,
    worker: &crate::config::Worker,
    core_cfg: &crate::config::AiCore,
    tools: Vec<String>,
    extra: Vec<Arc<dyn crate::ai::tools::Tool>>,
    instance_id: u64,
    event_tx: &mpsc::UnboundedSender<WorkflowEvent>,
) -> StepAction {
    let parts = build_worker_prompt_parts(worker);
    let session_id = Uuid::new_v4();

    let core = match build_workflow_core(core_cfg, session_id, parts, tools, extra) {
        Ok(c) => c,
        Err(e) => return StepAction::Fail { reason: e },
    };

    match tokio::time::timeout(
        WORKER_TURN_TIMEOUT,
        drive_turn(core, prompt, instance_id, event_tx),
    )
    .await
    {
        Ok(Ok(())) => StepAction::Advance,
        Ok(Err(e)) => StepAction::Fail { reason: e },
        Err(_) => StepAction::Fail {
            reason: format!(
                "worker turn timed out after {}s",
                WORKER_TURN_TIMEOUT.as_secs()
            ),
        },
    }
}

/// Drive one turn of a Conditional Worker step and observe the
/// `check_successful` signal. Returns `Advance` on `pass=true`, `Fail`
/// on `pass=false` or missing-call or turn error.
#[allow(clippy::too_many_arguments)]
async fn run_conditional_turn(
    instance_id: u64,
    _step_id: &str,
    prompt: &str,
    worker: &crate::config::Worker,
    core_cfg: &crate::config::AiCore,
    tools: Vec<String>,
    extra: Vec<Arc<dyn crate::ai::tools::Tool>>,
    sink_rx: &mut mpsc::UnboundedReceiver<
        crate::ai::tools::check_successful::CheckSuccessfulSignal,
    >,
    event_tx: &mpsc::UnboundedSender<WorkflowEvent>,
) -> StepAction {
    let parts = build_worker_prompt_parts(worker);
    let session_id = Uuid::new_v4();

    let core = match build_workflow_core(core_cfg, session_id, parts, tools, extra) {
        Ok(c) => c,
        Err(e) => return StepAction::Fail { reason: e },
    };

    // Drive the turn, then inspect the sink for an emitted signal.
    // Missing signal ⇒ failure per the design spec ("treat as pass=false").
    let turn_res = tokio::time::timeout(
        WORKER_TURN_TIMEOUT,
        drive_turn(core, prompt, instance_id, event_tx),
    )
    .await;

    // Collect the most-recent signal (tool can be called more than
    // once; the last call wins — most workers call it exactly once).
    let mut last_signal: Option<crate::ai::tools::check_successful::CheckSuccessfulSignal> = None;
    while let Ok(sig) = sink_rx.try_recv() {
        last_signal = Some(sig);
    }

    match turn_res {
        Ok(Ok(())) => {}
        Ok(Err(e)) => {
            return StepAction::Fail {
                reason: format!("worker turn failed: {e}"),
            };
        }
        Err(_) => {
            return StepAction::Fail {
                reason: format!(
                    "worker turn timed out after {}s",
                    WORKER_TURN_TIMEOUT.as_secs()
                ),
            };
        }
    }

    match last_signal {
        Some(sig) if sig.pass => StepAction::Advance,
        Some(sig) => StepAction::Fail {
            reason: sig
                .reason
                .unwrap_or_else(|| "check_successful(pass=false)".into()),
        },
        None => StepAction::Fail {
            reason: "worker did not call `check_successful` — treated as failure".into(),
        },
    }
}

/// Construct a workflow-scoped core. Only Anthropic / OpenAI variants
/// are supported here — CLI-harness cores are rejected earlier.
fn build_workflow_core(
    core_cfg: &crate::config::AiCore,
    session_id: Uuid,
    parts: crate::ai::claude_code::PromptParts,
    tools: Vec<String>,
    extra: Vec<Arc<dyn crate::ai::tools::Tool>>,
) -> Result<Box<dyn crate::ai::AICore>, String> {
    use crate::ai::anthropic_api::AnthropicApiCore;
    use crate::ai::openai_api::OpenAiApiCore;
    use crate::config::AiCoreKind;

    let boxed: Box<dyn crate::ai::AICore> = match &core_cfg.kind {
        AiCoreKind::AnthropicApi {
            api_url,
            api_key_env,
            model,
            system_prompt,
            ..
        } => {
            let c = AnthropicApiCore::new(
                session_id,
                api_url.clone(),
                api_key_env.clone(),
                model.clone(),
                parts,
                tools,
                system_prompt.clone(),
            )
            .with_extra_tools(extra);
            Box::new(c)
        }
        AiCoreKind::OpenAiApi {
            api_url,
            api_key_env,
            model,
            system_prompt,
            ..
        } => {
            let c = OpenAiApiCore::new(
                session_id,
                api_url.clone(),
                api_key_env.clone(),
                model.clone(),
                parts,
                tools,
                system_prompt.clone(),
            )
            .with_extra_tools(extra);
            Box::new(c)
        }
        _ => {
            return Err(
                "workflow cores must be anthropicApi or openAiApi (checked upstream)".into(),
            )
        }
    };
    Ok(boxed)
}

/// Minimal `PromptParts` for a workflow worker — just role and
/// instructions. Memory banks / channels / team context are omitted
/// because a workflow step is a one-shot execution rather than a
/// persistent agent session.
fn build_worker_prompt_parts(
    worker: &crate::config::Worker,
) -> crate::ai::claude_code::PromptParts {
    crate::ai::claude_code::PromptParts {
        role: worker
            .role_description
            .clone()
            .or(Some(worker.role.clone())),
        instructions: worker.instructions.clone(),
        ..Default::default()
    }
}

/// Drive a single turn on a constructed core, forwarding each
/// `AIEvent::Text` chunk as a `StepOutput` so the TUI tail shows the
/// worker's progress. Returns Ok(()) on `AIEvent::Done`.
async fn drive_turn(
    mut core: Box<dyn crate::ai::AICore>,
    prompt: &str,
    instance_id: u64,
    event_tx: &mpsc::UnboundedSender<WorkflowEvent>,
) -> Result<(), String> {
    let stream = core
        .turn(prompt)
        .await
        .map_err(|e| format!("turn failed: {e:?}"))?;
    tokio::pin!(stream);
    while let Some(ev) = stream.next().await {
        match ev {
            AIEvent::AssistantText { text, .. } => {
                for line in text.split('\n') {
                    if !line.is_empty() {
                        let _ = event_tx.send(WorkflowEvent::StepOutput {
                            instance_id,
                            line: line.to_string(),
                        });
                    }
                }
            }
            AIEvent::ToolUse { name, .. } => {
                let _ = event_tx.send(WorkflowEvent::StepOutput {
                    instance_id,
                    line: format!("[tool: {name}]"),
                });
            }
            AIEvent::Error { message, .. } => {
                return Err(message);
            }
            AIEvent::TurnResult { .. } => {
                // End-of-turn marker — treat as clean completion.
                return Ok(());
            }
            AIEvent::Stopped => {
                return Err("worker turn was stopped".into());
            }
            _ => {}
        }
    }
    // Stream ended without an explicit TurnResult — treat as success
    // (some cores shape termination differently).
    Ok(())
}

fn evaluate_check(value: Option<&PropertyValue>, op: CheckOp, expected: Option<&str>) -> bool {
    let rendered = value.map(|v| v.as_display()).unwrap_or_default();
    match op {
        CheckOp::Equals => expected.map(|e| rendered == e).unwrap_or(false),
        CheckOp::NotEquals => expected.map(|e| rendered != e).unwrap_or(true),
        CheckOp::Contains => {
            let Some(e) = expected else { return false };
            match value {
                Some(PropertyValue::Multi(items)) => items.iter().any(|i| i == e),
                _ => rendered.contains(e),
            }
        }
        CheckOp::Empty => rendered.is_empty(),
        CheckOp::NotEmpty => !rendered.is_empty(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_equals_matches_string() {
        assert!(evaluate_check(
            Some(&PropertyValue::Text("ok".into())),
            CheckOp::Equals,
            Some("ok")
        ));
        assert!(!evaluate_check(
            Some(&PropertyValue::Text("ok".into())),
            CheckOp::Equals,
            Some("fail")
        ));
    }

    #[test]
    fn check_not_equals_on_missing_is_true() {
        assert!(evaluate_check(None, CheckOp::NotEquals, Some("x")));
    }

    #[test]
    fn check_contains_on_multi_select_treats_items_as_list() {
        let v = PropertyValue::Multi(vec!["unit".into(), "integ".into()]);
        assert!(evaluate_check(Some(&v), CheckOp::Contains, Some("unit")));
        assert!(!evaluate_check(Some(&v), CheckOp::Contains, Some("e2e")));
    }

    #[test]
    fn check_empty_on_missing_variable() {
        assert!(evaluate_check(None, CheckOp::Empty, None));
        assert!(!evaluate_check(None, CheckOp::NotEmpty, None));
    }

    #[test]
    fn check_empty_on_empty_string() {
        let v = PropertyValue::Text(String::new());
        assert!(evaluate_check(Some(&v), CheckOp::Empty, None));
    }

    #[test]
    fn check_not_empty_on_populated_number() {
        let v = PropertyValue::Number(7.0);
        assert!(evaluate_check(Some(&v), CheckOp::NotEmpty, None));
    }

    use crate::config::{PropertyKind, StepKind, WorkflowProperty, WorkflowStep};
    use crate::workflow::event::{StepOutcome, WorkflowEvent};
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    fn minimal_config() -> AciddogConfig {
        AciddogConfig {
            team: crate::config::Team {
                id: "t".into(),
                description: "d".into(),
                replace_thinking_messages: false,
                default_ai_core: None,
            },
            agents: vec![],
            workers: vec![],
            automatons: vec![],
            ai_cores: vec![],
            memory_banks: vec![],
            compactions: vec![],
            channels: vec![],
            workflows: vec![],
            webhooks: vec![],
            thinking_messages: vec!["t".into()],
        }
    }

    fn one_command_workflow() -> Workflow {
        Workflow {
            id: "w".into(),
            description: "d".into(),
            max_retries: 1,
            properties: vec![WorkflowProperty {
                id: "msg".into(),
                label: None,
                required: false,
                kind: PropertyKind::Text {
                    default: Some("hi".into()),
                },
            }],
            steps: vec![WorkflowStep {
                id: "only".into(),
                kind: StepKind::Command {
                    command: "echo $$msg$$".into(),
                    cwd: None,
                },
            }],
        }
    }

    #[tokio::test]
    async fn command_step_runs_to_success() {
        let workflow = one_command_workflow();
        let mut vars = HashMap::new();
        vars.insert("msg".into(), PropertyValue::Text("hello".into()));
        let vars = Arc::new(Mutex::new(vars));
        let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel();
        let (_check_tx, check_rx) = tokio::sync::mpsc::unbounded_channel();
        let (_cancel_tx, cancel_rx) = tokio::sync::mpsc::channel::<()>(1);
        let config = Arc::new(minimal_config());

        run_instance(42, workflow, vars, config, event_tx, check_rx, cancel_rx).await;

        let mut finished_ok = None;
        let mut saw_output = false;
        while let Ok(ev) = event_rx.try_recv() {
            match ev {
                WorkflowEvent::StepOutput { line, .. } if line.contains("hello") => {
                    saw_output = true;
                }
                WorkflowEvent::StepFinished {
                    outcome: StepOutcome::Ok,
                    ..
                } => {}
                WorkflowEvent::Finished { ok, .. } => finished_ok = Some(ok),
                _ => {}
            }
        }
        assert!(saw_output, "expected stdout to carry 'hello'");
        assert_eq!(finished_ok, Some(true), "expected clean finish");
    }

    #[tokio::test]
    async fn check_step_retries_and_eventually_fails() {
        // Two steps: a no-op command, then a check against a variable
        // that's always "no". The check fails, we jump back to the
        // command, rerun, fail again — max_retries=1 so the second
        // failure terminates with Finished{ok:false}.
        let workflow = Workflow {
            id: "w".into(),
            description: "d".into(),
            max_retries: 1,
            properties: vec![WorkflowProperty {
                id: "state".into(),
                label: None,
                required: false,
                kind: PropertyKind::Hidden {
                    default: Some("no".into()),
                },
            }],
            steps: vec![
                WorkflowStep {
                    id: "noop".into(),
                    kind: StepKind::Command {
                        command: "true".into(),
                        cwd: None,
                    },
                },
                WorkflowStep {
                    id: "gate".into(),
                    kind: StepKind::Check {
                        variable: "state".into(),
                        op: CheckOp::Equals,
                        value: Some("yes".into()),
                        target: "noop".into(),
                    },
                },
            ],
        };
        let mut vars = HashMap::new();
        vars.insert("state".into(), PropertyValue::Text("no".into()));
        let vars = Arc::new(Mutex::new(vars));
        let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel();
        let (_check_tx, check_rx) = tokio::sync::mpsc::unbounded_channel();
        let (_cancel_tx, cancel_rx) = tokio::sync::mpsc::channel::<()>(1);
        let config = Arc::new(minimal_config());
        run_instance(7, workflow, vars, config, event_tx, check_rx, cancel_rx).await;

        let mut saw_retry = false;
        let mut finished_ok = None;
        while let Ok(ev) = event_rx.try_recv() {
            match ev {
                WorkflowEvent::Retrying { .. } => saw_retry = true,
                WorkflowEvent::Finished { ok, .. } => finished_ok = Some(ok),
                _ => {}
            }
        }
        assert!(
            saw_retry,
            "expected a Retrying event after the first gate failure"
        );
        assert_eq!(
            finished_ok,
            Some(false),
            "expected terminal failure after retries exhausted"
        );
    }
}
