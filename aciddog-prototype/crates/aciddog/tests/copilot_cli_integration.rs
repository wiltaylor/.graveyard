//! Live integration against the real `copilot` CLI.
//!
//! The live test is `#[ignore]` by default so `cargo test` passes on
//! machines without copilot on PATH. It also checks for the binary
//! explicitly so `cargo test -- --ignored` can no-op cleanly on
//! machines where copilot simply isn't installed.
//!
//! Run explicitly with: `cargo test --test copilot_cli_integration -- --ignored`

use aciddog::ai::{AICore, AIEvent, CopilotCliCore, PromptParts};
use futures::StreamExt;
use std::process::Command;
use uuid::Uuid;

fn parts(instructions: &str) -> PromptParts {
    PromptParts {
        instructions: instructions.into(),
        ..PromptParts::default()
    }
}

/// `true` when the `copilot` binary is invokable on PATH. The live
/// test skips with a printed note rather than failing when it isn't.
fn copilot_available() -> bool {
    Command::new("copilot")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[tokio::test]
#[ignore]
async fn live_turn_streams_events_and_reuses_session() {
    if !copilot_available() {
        eprintln!("skipping: `copilot` binary not found on PATH");
        return;
    }

    let expected_session = Uuid::new_v4();
    let mut core = CopilotCliCore::new(
        expected_session,
        parts("Respond with a single word."),
        vec!["Read".into()],
    );
    assert_eq!(core.session_id(), Some(expected_session));

    let mut stream = core
        .turn("Reply with exactly the word READY and nothing else.")
        .await
        .expect("spawn copilot");

    let mut saw_init = false;
    let mut saw_text = false;
    let mut saw_result = false;

    while let Some(ev) = stream.next().await {
        match ev {
            AIEvent::SystemInit { session_id, .. } => {
                assert_eq!(session_id, expected_session);
                saw_init = true;
            }
            AIEvent::AssistantText { .. } => saw_text = true,
            AIEvent::TurnResult { .. } => saw_result = true,
            AIEvent::Error { message, .. } => panic!("unexpected error event: {message}"),
            _ => {}
        }
    }

    assert!(saw_init, "expected a SystemInit event");
    assert!(saw_text, "expected at least one AssistantText event");
    assert!(saw_result, "expected a TurnResult event");

    // Second turn reuses the same session id.
    let mut stream2 = core
        .turn("What word did you just say?")
        .await
        .expect("second turn");
    let mut saw_second_init = false;
    while let Some(ev) = stream2.next().await {
        if let AIEvent::SystemInit { session_id, .. } = ev {
            assert_eq!(session_id, expected_session);
            saw_second_init = true;
        }
    }
    assert!(
        saw_second_init,
        "second turn should re-emit SystemInit with same session id"
    );
}

#[tokio::test]
async fn missing_binary_yields_spawn_error_event() {
    // Spawn failures surface as AIEvent::Error inside the returned
    // stream (the synthesized SystemInit still arrives first). The
    // error wording must reference the failed spawn path.
    let mut core = CopilotCliCore::new(Uuid::new_v4(), parts("i"), vec![])
        .with_binary("/no/such/path/copilot");
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
