use aciddog::tui::test_support::worker_archive_roundtrip;

#[tokio::test]
async fn worker_force_archive_tombstones_registry_surfaces() {
    let result = worker_archive_roundtrip().await;

    assert!(result.archive_response.ok, "{:?}", result.archive_response);
    assert_eq!(result.archive_response.message.as_deref(), Some("archived"));

    let worker_list = result
        .worker_list_response
        .message
        .as_deref()
        .unwrap_or_default();
    assert!(
        !worker_list.contains(&result.worker_id),
        "worker list leaked archived id: {worker_list}"
    );

    assert!(!result.agent_status_response.ok);
    assert!(
        result
            .agent_status_response
            .error
            .as_deref()
            .unwrap_or_default()
            .contains("no agent named"),
        "{:?}",
        result.agent_status_response
    );

    let agent_list = result
        .agent_list_response
        .message
        .as_deref()
        .unwrap_or_default();
    assert!(
        !agent_list.contains(&result.worker_id),
        "agent list leaked archived id: {agent_list}"
    );

    assert!(result.archived);
    assert!(!result.agent_id_present);
}
