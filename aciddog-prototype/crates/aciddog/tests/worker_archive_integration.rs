use aciddog::tui::test_support::{
    cascade_helper_mixed_descendants, force_archive_with_force_stop_children,
    parent_archive_cascade_hibernated_child, parent_archive_defers_working_child,
    prune_orphans_archives_existing_orphans, worker_archive_roundtrip,
    worker_self_archives_when_mailing_archived_parent as archived_parent_mail_scenario,
};

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

#[tokio::test]
async fn parent_archive_cascades_to_hibernated_child() {
    let result = parent_archive_cascade_hibernated_child().await;

    assert_eq!(
        result.archive_response.message.as_deref(),
        Some("archived (cascade-archived 1 children: B)")
    );
    assert!(result.parent_archived);
    assert!(result.child_archived);
    assert!(!result.parent_agent_id_present);
    assert!(!result.child_agent_id_present);

    let worker_list = result
        .worker_list_response
        .message
        .as_deref()
        .unwrap_or_default();
    assert!(!worker_list.contains(&result.child_id));

    let agent_list = result
        .agent_list_response
        .message
        .as_deref()
        .unwrap_or_default();
    assert!(!agent_list.contains(&result.child_id));

    assert!(!result.agent_status_response.ok);
    assert!(result
        .agent_status_response
        .error
        .as_deref()
        .unwrap_or_default()
        .contains("no agent named"));
}

#[tokio::test]
async fn parent_archive_defers_working_child_via_archive_when_done() {
    let result = parent_archive_defers_working_child().await;

    assert_eq!(result.archive_response.message.as_deref(), Some("archived"));
    assert!(result.parent_archived);
    assert!(!result.child_archived_after_archive);
    assert!(result.child_archive_when_done_after_archive);
    assert!(result.complete_response.ok);
    assert!(result.child_archived_after_complete);
}

#[tokio::test]
async fn force_archive_with_force_stop_children_tombstones_immediately() {
    let result = force_archive_with_force_stop_children().await;

    assert_eq!(
        result.archive_response.message.as_deref(),
        Some("archived (cascade-archived 1 children: B)")
    );
    assert!(result.parent_archived);
    assert!(result.child_archived);
    assert!(!result.child_archive_when_done);
    assert_eq!(result.cancel_signals, 1);
}

#[test]
fn cascade_helper_handles_mixed_descendants_correctly() {
    let result = cascade_helper_mixed_descendants();

    assert!(result.deferred_child_archive_when_done);
    assert!(result.deferred_child_archived);
    assert!(result.hibernated_grandchild_archived);
    assert!(result.ready_child_archived);
    assert_eq!(result.first_summary, vec!["D".to_string(), "C".to_string()]);
    assert!(result.second_summary.is_empty());
}

#[tokio::test]
async fn prune_orphans_archives_existing_orphans_without_restart() {
    let result = prune_orphans_archives_existing_orphans().await;

    assert_eq!(
        result.first_response.message.as_deref(),
        Some("pruned 1 orphans: B")
    );
    assert!(result.child_archived);
    assert_eq!(
        result.second_response.message.as_deref(),
        Some("no orphans found")
    );
}

#[tokio::test]
async fn worker_self_archives_when_mailing_archived_parent() {
    let result = archived_parent_mail_scenario().await;

    assert!(!result.response.ok);
    assert!(result
        .response
        .error
        .as_deref()
        .unwrap_or_default()
        .contains("parent has been archived"));
    assert!(result.worker_archived);
    assert!(!result.worker_agent_id_present);
}
