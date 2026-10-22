use aciddog_manager::db;
use aciddog_manager::model::instance::{self, CreateInstance, Transport};

#[tokio::test]
async fn insert_list_delete_round_trip() {
    let pool = db::connect_memory().await.expect("in-memory pool");

    let created = instance::insert(
        &pool,
        CreateInstance {
            name: "local".into(),
            transport: Transport::Unix,
            url: None,
            socket_path: Some("/tmp/aciddog-x.sock".into()),
            api_key: None,
            tls_ca_path: None,
        },
    )
    .await
    .expect("insert ok");

    assert_eq!(created.name, "local");
    assert!(!created.has_api_key);
    assert_eq!(created.transport, Transport::Unix);

    let listed = instance::list(&pool).await.expect("list ok");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, created.id);
    assert!(!listed[0].has_api_key, "list must omit api_key");

    instance::delete(&pool, created.id)
        .await
        .expect("delete ok");
    let listed = instance::list(&pool).await.expect("list ok");
    assert!(listed.is_empty(), "deleted row should be gone");
}

#[tokio::test]
async fn duplicate_name_rejected() {
    let pool = db::connect_memory().await.unwrap();
    let payload = || CreateInstance {
        name: "dupe".into(),
        transport: Transport::Unix,
        url: None,
        socket_path: Some("/tmp/a.sock".into()),
        api_key: None,
        tls_ca_path: None,
    };
    instance::insert(&pool, payload()).await.unwrap();
    let err = instance::insert(&pool, payload()).await.unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("already exists"), "got: {msg}");
}

#[tokio::test]
async fn https_requires_url() {
    let pool = db::connect_memory().await.unwrap();
    let err = instance::insert(
        &pool,
        CreateInstance {
            name: "bad".into(),
            transport: Transport::Https,
            url: None,
            socket_path: None,
            api_key: Some("k".into()),
            tls_ca_path: None,
        },
    )
    .await
    .unwrap_err();
    assert!(format!("{err}").contains("url is required"));
}

#[tokio::test]
async fn unix_rejects_api_key() {
    let pool = db::connect_memory().await.unwrap();
    let err = instance::insert(
        &pool,
        CreateInstance {
            name: "bad".into(),
            transport: Transport::Unix,
            url: None,
            socket_path: Some("/tmp/a.sock".into()),
            api_key: Some("k".into()),
            tls_ca_path: None,
        },
    )
    .await
    .unwrap_err();
    assert!(format!("{err}").contains("api_key"));
}

#[tokio::test]
async fn delete_unknown_is_404() {
    let pool = db::connect_memory().await.unwrap();
    let err = instance::delete(&pool, uuid::Uuid::new_v4())
        .await
        .unwrap_err();
    assert!(matches!(err, aciddog_manager::error::AppError::NotFound));
}
