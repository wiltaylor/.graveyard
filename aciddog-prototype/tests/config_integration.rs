use std::path::{Path, PathBuf};

use aciddog::config::{
    load_config, parse_config, AiCoreKind, AutomatonBackend, ConfigError, RemoteTransport,
};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn loads_demo_config_end_to_end() {
    let path = repo_root().join("demo").join("aciddog.wcl");
    let cfg = load_config(&path).expect("demo aciddog.wcl should parse");

    assert_eq!(cfg.team.id, "default");
    assert!(cfg.agents.iter().any(|a| a.id == "planner"));
    assert!(cfg.agents.iter().any(|a| a.id == "coder"));
    for agent in &cfg.agents {
        assert!(!agent.role.is_empty());
        assert!(!agent.instructions.is_empty());
        // Agents may leave `model` unset; the resolved ai_core's
        // `model` (or the child CLI's own default) kicks in at
        // core-construction time.
        if let Some(m) = agent.model.as_deref() {
            assert!(!m.is_empty());
        }
    }

    // Workers declared in the demo config should parse and be referenced
    // correctly by each other and by the coder agent.
    assert!(cfg.workers.iter().any(|w| w.id == "researcher"));
    assert!(cfg.workers.iter().any(|w| w.id == "fetcher"));
    assert!(cfg.can_create("coder", "researcher"));
    assert!(cfg.can_create("researcher", "fetcher"));
    assert!(!cfg.can_create("planner", "researcher"));
}

#[test]
fn demo_declares_z_ai_anthropic_core_bound_to_tester() {
    let path = repo_root().join("demo").join("aciddog.wcl");
    let cfg = load_config(&path).expect("demo aciddog.wcl should parse");

    let z = cfg
        .ai_cores
        .iter()
        .find(|c| c.id == "z_ai")
        .expect("z_ai core should be declared");
    match &z.kind {
        AiCoreKind::AnthropicApi {
            api_url,
            api_key_env,
            model,
            ..
        } => {
            assert_eq!(api_url, "https://api.z.ai/api/anthropic");
            assert_eq!(api_key_env, "ZAI_API_KEY");
            assert_eq!(model, "glm-5.1");
        }
        other => panic!("expected AnthropicApi variant, got {other:?}"),
    }

    let tester = cfg
        .agents
        .iter()
        .find(|a| a.id == "tester")
        .expect("tester agent");
    assert_eq!(tester.ai_core.as_deref(), Some("z_ai"));
    assert_eq!(
        tester.tools.as_deref(),
        Some(
            &[
                "Bash".to_string(),
                "Read".to_string(),
                "Grep".to_string(),
                "Glob".to_string(),
            ][..]
        )
    );

    // `coder` stays on the default (ClaudeCode) core so both code
    // paths run side by side in the demo.
    let coder = cfg
        .agents
        .iter()
        .find(|a| a.id == "coder")
        .expect("coder agent");
    assert_eq!(coder.ai_core, None);
}

#[test]
fn demo_declares_openrouter_openai_core_bound_to_jokemaster() {
    let path = repo_root().join("demo").join("aciddog.wcl");
    let cfg = load_config(&path).expect("demo aciddog.wcl should parse");

    let openrouter = cfg
        .ai_cores
        .iter()
        .find(|c| c.id == "openrouter")
        .expect("openrouter core should be declared");
    match &openrouter.kind {
        AiCoreKind::OpenAiApi {
            api_url,
            api_key_env,
            model,
            ..
        } => {
            assert_eq!(api_url, "https://openrouter.ai/api/v1");
            assert_eq!(api_key_env, "OPENROUTER_API_KEY");
            assert_eq!(model, "openai/gpt-5.4");
        }
        other => panic!("expected OpenAiApi variant, got {other:?}"),
    }

    let jokemaster = cfg
        .agents
        .iter()
        .find(|a| a.id == "jokemaster")
        .expect("jokemaster agent");
    assert_eq!(jokemaster.ai_core.as_deref(), Some("openrouter"));
    assert_eq!(jokemaster.tools.as_deref(), Some(&["Read".to_string()][..]));
}

#[test]
fn demo_declares_copilot_haiku_core_bound_to_shitposter() {
    let path = repo_root().join("demo").join("aciddog.wcl");
    let cfg = load_config(&path).expect("demo aciddog.wcl should parse");

    let core = cfg
        .ai_cores
        .iter()
        .find(|c| c.id == "copilot_haiku")
        .expect("copilot_haiku core should be declared");
    match &core.kind {
        AiCoreKind::CopilotCli {
            model,
            default_tools,
        } => {
            assert_eq!(model.as_deref(), Some("claude-haiku-4.5"));
            assert_eq!(default_tools.as_deref(), Some(&["Read".to_string()][..]));
        }
        other => panic!("expected CopilotCli variant, got {other:?}"),
    }

    let shitposter = cfg
        .agents
        .iter()
        .find(|a| a.id == "shitposter")
        .expect("shitposter agent");
    assert_eq!(shitposter.ai_core.as_deref(), Some("copilot_haiku"));
    assert_eq!(shitposter.tools.as_deref(), Some(&["Read".to_string()][..]));
}

#[test]
fn demo_declares_project_context_memory_bank_and_wires_it() {
    let path = repo_root().join("demo").join("aciddog.wcl");
    let cfg = load_config(&path).expect("demo aciddog.wcl should parse");

    let bank = cfg
        .memory_bank("project_context")
        .expect("demo should declare project_context bank");
    assert!(bank.description.to_lowercase().contains("project"));
    assert!(cfg.can_access_bank("planner", "project_context"));
    assert!(cfg.can_access_bank("coder", "project_context"));
    // jokester was never granted access; must default to denied.
    assert!(!cfg.can_access_bank("jokester", "project_context"));
}

#[test]
fn memory_bank_block_parses_with_optional_embed_model() {
    let src = r#"
        team default { description = "t" }
        memory_bank vault {
          description = "shared notes"
          embed_model = "BGE-base-en-v1.5"
        }
        agent alpha {
          role = "r"
          instructions = "i"
          memory_banks = ["vault"]
        }
    "#;
    let cfg = parse_config(src, Path::new("<test>")).expect("parses");
    assert_eq!(cfg.memory_banks.len(), 1);
    let bank = &cfg.memory_banks[0];
    assert_eq!(bank.id, "vault");
    assert_eq!(bank.embed_model.as_deref(), Some("BGE-base-en-v1.5"));
    assert!(cfg.can_access_bank("alpha", "vault"));
}

#[test]
fn unknown_memory_bank_ref_errors() {
    let src = r#"
        team default { description = "t" }
        agent alpha {
          role = "r"
          instructions = "i"
          memory_banks = ["ghost"]
        }
    "#;
    let err = parse_config(src, Path::new("<test>")).expect_err("should fail");
    assert!(
        matches!(err, ConfigError::UnknownMemoryBankRef { ref from, ref to } if from == "alpha" && to == "ghost"),
        "got {err:?}"
    );
}

#[test]
fn loads_remote_demo_server_config() {
    let path = repo_root()
        .join("demo-remote-server")
        .join("aciddog.wcl");
    let cfg = load_config(&path).expect("demo-remote-server/aciddog.wcl should parse");
    assert_eq!(cfg.team.id, "server");
    assert!(
        cfg.agents.iter().any(|a| a.id == "harbor"),
        "harbor agent declared"
    );
    let gateway = cfg
        .automatons
        .iter()
        .find(|a| a.id == "gateway")
        .expect("gateway automaton declared");
    match &gateway.backend {
        AutomatonBackend::Webservice { .. } => {}
        other => panic!("expected webservice backend, got {other:?}"),
    }
    assert!(
        gateway
            .can_message
            .as_ref()
            .is_some_and(|v| v.iter().any(|id| id == "harbor")),
        "gateway.can_message should include harbor so inbound RPCs reach it"
    );
}

#[test]
fn loads_remote_demo_client_config() {
    let path = repo_root()
        .join("demo-remote-client")
        .join("aciddog.wcl");
    let cfg = load_config(&path).expect("demo-remote-client/aciddog.wcl should parse");
    assert_eq!(cfg.team.id, "client");
    assert!(
        cfg.agents.iter().any(|a| a.id == "scout"),
        "scout agent declared"
    );
    let mothership = cfg
        .automatons
        .iter()
        .find(|a| a.id == "mothership")
        .expect("mothership automaton declared");
    match &mothership.backend {
        AutomatonBackend::Remote {
            remote_target,
            deliver_to,
            transport,
            ..
        } => {
            assert_eq!(remote_target, "harbor");
            assert_eq!(deliver_to, "scout");
            match transport {
                RemoteTransport::UnixSocket { path } => {
                    assert!(path.to_string_lossy().ends_with("aciddog-remote-demo.sock"));
                }
                other => panic!("expected UnixSocket transport, got {other:?}"),
            }
        }
        other => panic!("expected remote backend, got {other:?}"),
    }
    assert!(
        mothership
            .can_message
            .as_ref()
            .is_some_and(|v| v.iter().any(|id| id == "scout")),
        "mothership.can_message should include scout so inbound SSE deliveries succeed"
    );
}

#[test]
fn remote_automaton_parses_with_unix_socket() {
    let src = r#"
        team default { description = "t" }
        agent planner {
          role = "p"
          instructions = "i"
        }
        automaton peer {
          type          = :remote
          role          = "bridge"
          socket_path   = "/tmp/remote.sock"
          remote_target = "bridge"
          deliver_to    = "planner"
        }
    "#;
    let cfg = parse_config(src, Path::new("<test>")).expect("parses");
    let a = &cfg.automatons[0];
    assert_eq!(a.id, "peer");
    match &a.backend {
        AutomatonBackend::Remote {
            transport,
            remote_target,
            deliver_to,
            proxy_socket_path,
        } => {
            match transport {
                RemoteTransport::UnixSocket { path } => {
                    assert_eq!(path.to_str(), Some("/tmp/remote.sock"));
                }
                other => panic!("unexpected transport: {other:?}"),
            }
            assert_eq!(remote_target, "bridge");
            assert_eq!(deliver_to, "planner");
            assert!(proxy_socket_path.is_none());
        }
        other => panic!("unexpected backend: {other:?}"),
    }
}

#[test]
fn remote_automaton_parses_with_https() {
    let src = r#"
        team default { description = "t" }
        agent planner {
          role = "p"
          instructions = "i"
        }
        automaton peer {
          type          = :remote
          role          = "bridge"
          url           = "https://host:8443"
          api_key_env   = "REMOTE_TOKEN"
          remote_target = "bridge"
          deliver_to    = "planner"
          proxy_socket_path = "/tmp/proxy.sock"
        }
    "#;
    let cfg = parse_config(src, Path::new("<test>")).expect("parses");
    let a = &cfg.automatons[0];
    match &a.backend {
        AutomatonBackend::Remote {
            transport,
            proxy_socket_path,
            ..
        } => {
            match transport {
                RemoteTransport::Https {
                    url,
                    api_key_env,
                    tls_ca,
                } => {
                    assert_eq!(url, "https://host:8443");
                    assert_eq!(api_key_env, "REMOTE_TOKEN");
                    assert!(tls_ca.is_none());
                }
                other => panic!("unexpected transport: {other:?}"),
            }
            assert_eq!(
                proxy_socket_path.as_ref().and_then(|p| p.to_str()),
                Some("/tmp/proxy.sock")
            );
        }
        other => panic!("unexpected backend: {other:?}"),
    }
}

#[test]
fn remote_automaton_rejects_both_transports() {
    let src = r#"
        team default { description = "t" }
        agent planner { role = "p" instructions = "i" }
        automaton peer {
          type          = :remote
          role          = "bridge"
          url           = "https://host:8443"
          api_key_env   = "REMOTE_TOKEN"
          socket_path   = "/tmp/remote.sock"
          remote_target = "bridge"
          deliver_to    = "planner"
        }
    "#;
    let err = parse_config(src, Path::new("<test>")).expect_err("should fail");
    assert!(
        matches!(err, ConfigError::RemoteTransportConflict { .. }),
        "got {err:?}"
    );
}

#[test]
fn remote_automaton_rejects_https_without_api_key() {
    let src = r#"
        team default { description = "t" }
        agent planner { role = "p" instructions = "i" }
        automaton peer {
          type          = :remote
          role          = "bridge"
          url           = "https://host:8443"
          remote_target = "bridge"
          deliver_to    = "planner"
        }
    "#;
    let err = parse_config(src, Path::new("<test>")).expect_err("should fail");
    assert!(
        matches!(err, ConfigError::RemoteMissingApiKeyEnv { .. }),
        "got {err:?}"
    );
}

#[test]
fn remote_automaton_rejects_api_key_on_unix_socket() {
    let src = r#"
        team default { description = "t" }
        agent planner { role = "p" instructions = "i" }
        automaton peer {
          type          = :remote
          role          = "bridge"
          socket_path   = "/tmp/remote.sock"
          api_key_env   = "REMOTE_TOKEN"
          remote_target = "bridge"
          deliver_to    = "planner"
        }
    "#;
    let err = parse_config(src, Path::new("<test>")).expect_err("should fail");
    assert!(
        matches!(err, ConfigError::RemoteUnixSocketApiKey { .. }),
        "got {err:?}"
    );
}

#[test]
fn remote_automaton_rejects_unknown_deliver_to() {
    let src = r#"
        team default { description = "t" }
        agent planner { role = "p" instructions = "i" }
        automaton peer {
          type          = :remote
          role          = "bridge"
          socket_path   = "/tmp/remote.sock"
          remote_target = "bridge"
          deliver_to    = "ghost"
        }
    "#;
    let err = parse_config(src, Path::new("<test>")).expect_err("should fail");
    assert!(
        matches!(err, ConfigError::RemoteUnknownDeliverTo { ref target, .. } if target == "ghost"),
        "got {err:?}"
    );
}

#[test]
fn remote_automaton_rejects_missing_transport() {
    let src = r#"
        team default { description = "t" }
        agent planner { role = "p" instructions = "i" }
        automaton peer {
          type          = :remote
          role          = "bridge"
          remote_target = "bridge"
          deliver_to    = "planner"
        }
    "#;
    let err = parse_config(src, Path::new("<test>")).expect_err("should fail");
    assert!(
        matches!(err, ConfigError::RemoteMissingTransport { .. }),
        "got {err:?}"
    );
}

#[test]
fn script_automaton_rejects_remote_fields() {
    let src = r#"
        team default { description = "t" }
        automaton bot {
          role          = "r"
          command       = "echo"
          remote_target = "bridge"
        }
    "#;
    let err = parse_config(src, Path::new("<test>")).expect_err("should fail");
    assert!(
        matches!(
            err,
            ConfigError::ScriptUnexpectedWebserviceField { ref field, .. } if field == "remote_target"
        ),
        "got {err:?}"
    );
}
