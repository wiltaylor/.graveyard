//! Live integration against the real `gemini` CLI.
//!
//! The live test is `#[ignore]` by default so `cargo test` passes on
//! machines without gemini on PATH. It also checks for the binary
//! explicitly so `cargo test -- --ignored` can no-op cleanly on
//! machines where gemini simply isn't installed or authenticated.
//!
//! Run explicitly with: `cargo test --test gemini_cli_integration -- --ignored`

use aciddog::ai::{AICore, AIEvent, GeminiCliCore, PromptParts};
use futures::StreamExt;
use std::process::Command;
use uuid::Uuid;

fn parts(instructions: &str) -> PromptParts {
    PromptParts {
        instructions: instructions.into(),
        ..PromptParts::default()
    }
}

/// `true` when the `gemini` binary is invokable on PATH. The live
/// test skips with a printed note rather than failing when it isn't.
fn gemini_available() -> bool {
    Command::new("gemini")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[tokio::test]
#[ignore]
async fn live_turn_streams_events_and_reuses_session() {
    if !gemini_available() {
        eprintln!("skipping: `gemini` binary not found on PATH");
        return;
    }

    let aciddog_session = Uuid::new_v4();
    let mut core = GeminiCliCore::new(
        aciddog_session,
        parts("Respond with a single word."),
        vec!["Read".into()],
    );
    assert_eq!(core.session_id(), Some(aciddog_session));

    let mut stream = core
        .turn("Reply with exactly the word READY and nothing else.")
        .await
        .expect("spawn gemini");

    let mut saw_synth_init = false;
    let mut saw_real_init = false;
    let mut first_gemini_session: Option<Uuid> = None;
    let mut saw_text = false;
    let mut saw_result = false;

    while let Some(ev) = stream.next().await {
        match ev {
            AIEvent::SystemInit { session_id, .. } => {
                // First SystemInit uses the aciddog-side id (synthetic);
                // the second carries gemini's real UUID from its own
                // `init` stream event.
                if !saw_synth_init {
                    assert_eq!(session_id, aciddog_session);
                    saw_synth_init = true;
                } else {
                    first_gemini_session = Some(session_id);
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
    assert!(saw_real_init, "expected gemini's own init to surface");
    assert!(saw_text, "expected at least one AssistantText event");
    assert!(saw_result, "expected a TurnResult event");

    // Second turn reuses gemini's captured session id — verify by
    // watching for the same gemini uuid in the real-init event.
    let mut stream2 = core
        .turn("What word did you just say?")
        .await
        .expect("second turn");
    let mut saw_same_gemini_session = false;
    let mut second_turn_real_init_seen = false;
    let mut saw_synth_on_second = false;
    while let Some(ev) = stream2.next().await {
        if let AIEvent::SystemInit { session_id, .. } = ev {
            if !saw_synth_on_second {
                assert_eq!(session_id, aciddog_session);
                saw_synth_on_second = true;
            } else {
                second_turn_real_init_seen = true;
                if Some(session_id) == first_gemini_session {
                    saw_same_gemini_session = true;
                }
            }
        }
    }
    assert!(
        second_turn_real_init_seen,
        "second turn should also emit gemini's real init"
    );
    assert!(
        saw_same_gemini_session,
        "second turn should reuse the same gemini session id captured on turn 1"
    );
}

#[tokio::test]
async fn missing_binary_yields_spawn_error_event() {
    // Spawn failures surface as AIEvent::Error inside the returned
    // stream (the synthesized SystemInit still arrives first). The
    // error wording must reference the failed spawn path.
    let mut core =
        GeminiCliCore::new(Uuid::new_v4(), parts("i"), vec![]).with_binary("/no/such/path/gemini");
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
