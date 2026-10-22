//! Live integration against the real `codex` CLI.
//!
//! The live test is `#[ignore]` by default so `cargo test` passes on
//! machines without codex on PATH. It also checks for the binary
//! explicitly so `cargo test -- --ignored` can no-op cleanly on
//! machines where codex simply isn't installed or authenticated.
//!
//! Run explicitly with: `cargo test --test codex_cli_integration -- --ignored`

use aciddog::ai::{AICore, AIEvent, CodexCliCore, PromptParts};
use futures::StreamExt;
use std::process::Command;
use uuid::Uuid;

fn parts(instructions: &str) -> PromptParts {
    PromptParts {
        instructions: instructions.into(),
        ..PromptParts::default()
    }
}

/// `true` when the `codex` binary is invokable on PATH. The live test
/// skips with a printed note rather than failing when it isn't.
fn codex_available() -> bool {
    Command::new("codex")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[tokio::test]
#[ignore]
async fn live_turn_streams_events_and_captures_session_id() {
    if !codex_available() {
        eprintln!("skipping: `codex` binary not found on PATH");
        return;
    }

    let aciddog_session = Uuid::new_v4();
    let mut core = CodexCliCore::new(
        aciddog_session,
        parts("Respond with a single word."),
        vec!["Read".into()],
    );
    assert_eq!(core.session_id(), Some(aciddog_session));

    let mut stream = core
        .turn("Reply with exactly the word READY and nothing else.")
        .await
        .expect("spawn codex");

    let mut saw_synth_init = false;
    let mut saw_real_init = false;
    let mut first_codex_thread: Option<Uuid> = None;
    let mut saw_text = false;
    let mut saw_result = false;

    while let Some(ev) = stream.next().await {
        match ev {
            AIEvent::SystemInit { session_id, .. } => {
                // First SystemInit uses the aciddog-side id
                // (synthetic); the second carries codex's real
                // thread id from `thread.started`.
                if !saw_synth_init {
                    assert_eq!(session_id, aciddog_session);
                    saw_synth_init = true;
                } else {
                    first_codex_thread = Some(session_id);
                    saw_real_init = true;
                }
            }
            AIEvent::AssistantText { .. } => saw_text = true,
            AIEvent::TurnResult { .. } => saw_result = true,
            AIEvent::Error { message, .. } => panic!("unexpected error event: {message}"),
            _ => {}
        }
    }

    assert!(saw_synth_init, "expected a synthesized SystemInit event");
    assert!(saw_real_init, "expected codex's thread.started to surface");
    assert!(
        first_codex_thread.is_some(),
        "expected to capture a codex thread UUID"
    );
    assert!(saw_text, "expected at least one AssistantText event");
    assert!(saw_result, "expected a TurnResult event");
}

#[tokio::test]
async fn missing_binary_yields_spawn_error_event() {
    // Spawn failures surface as AIEvent::Error inside the returned
    // stream (the synthesized SystemInit still arrives first). The
    // error wording must reference the failed spawn path.
    let mut core =
        CodexCliCore::new(Uuid::new_v4(), parts("i"), vec![]).with_binary("/no/such/path/codex");
    let mut stream = core
        .turn("hi")
        .await
        .expect("turn() now always returns Ok; errors arrive via stream");
    let mut err_msg = None;
    while let Some(ev) = stream.next().await {
        if let AIEvent::Error { message, .. } = ev {
            err_msg = Some(message);
            break;
        }
    }
    let msg = err_msg.expect("expected AIEvent::Error in stream");
    assert!(
        msg.contains("not found") || msg.contains("spawn") || msg.contains("No such file"),
        "unexpected error message: {msg}"
    );
}
