#![forbid(unsafe_code)]
#![cfg(feature = "test-fixtures")]

use slate_broadwebd::{
    BroadwebDaemon, BroadwebdError, HttpFetchRequest, HttpHeader, ProfileSyncObjectRequest,
    ProfileSyncPutObjectRequest, ProfileSyncRequest, ProfileSyncResponse, ProfileSyncRootRequest,
    ProfileSyncRootUpdate, ResourceBudget,
    test_fixtures::{InProcessBroadwebNetwork, InternalFixtureHttpResponse},
};

#[test]
fn daemon_fetches_fixture_http_without_loopback_transport() {
    let network = InProcessBroadwebNetwork::new();
    let fixture = network.http_response(InternalFixtureHttpResponse {
        status_code: 200,
        content_type: Some("text/html; charset=utf-8".to_string()),
        headers: vec![HttpHeader {
            name: "content-type".to_string(),
            value: "text/html; charset=utf-8".to_string(),
        }],
        body: b"<!doctype html><title>In Process Fixture</title><h1>Fixture body</h1>".to_vec(),
    });
    let fixture_url = fixture.base_url().to_string();
    let state_root = std::env::temp_dir().join(format!(
        "slate-broadwebd-in-process-fixture-{}",
        std::process::id()
    ));
    let daemon = BroadwebDaemon::start_with_registry(
        &state_root,
        ResourceBudget::default(),
        network.fixture_registry(),
    )
    .expect("daemon");

    let response = daemon
        .fetch_http(HttpFetchRequest::default_profile(&fixture_url))
        .expect("fetch synthetic fixture URL");
    let requests = fixture.finish();

    assert!(fixture_url.starts_with("slate-fixture-http://"));
    assert_eq!(response.status_code, 200);
    assert!(response.body_text_lossy().contains("Fixture body"));
    assert_eq!(requests, vec!["/"]);

    let _ = std::fs::remove_dir_all(state_root);
}

#[test]
fn fixture_registry_rejects_external_http_without_network_access() {
    let network = InProcessBroadwebNetwork::new();
    let state_root = std::env::temp_dir().join(format!(
        "slate-broadwebd-fixture-external-reject-{}",
        std::process::id()
    ));
    let daemon = BroadwebDaemon::start_with_registry(
        &state_root,
        ResourceBudget::default(),
        network.fixture_registry(),
    )
    .expect("daemon");

    let error = daemon
        .fetch_http(HttpFetchRequest::default_profile("https://example.com/"))
        .expect_err("fixture-only transport must reject external HTTP");

    assert!(matches!(
        error,
        BroadwebdError::UnsupportedRequest(message)
            if message.contains("cannot fetch external URL")
    ));

    let _ = std::fs::remove_dir_all(state_root);
}

#[test]
fn device_daemons_share_profile_sync_without_loopback_transport() {
    let network = InProcessBroadwebNetwork::new();
    let first_state_root = std::env::temp_dir().join(format!(
        "slate-broadwebd-device-fixture-first-{}",
        std::process::id()
    ));
    let second_state_root = std::env::temp_dir().join(format!(
        "slate-broadwebd-device-fixture-second-{}",
        std::process::id()
    ));
    let first = network
        .daemon_for_device(&first_state_root, ResourceBudget::default(), "fixture-a")
        .expect("start first in-process device daemon");
    let second = network
        .daemon_for_device(&second_state_root, ResourceBudget::default(), "fixture-b")
        .expect("start second in-process device daemon");

    let ProfileSyncResponse::PutEncryptedObject { object_id } = first
        .profile_sync(ProfileSyncRequest::PutEncryptedObject(
            ProfileSyncPutObjectRequest::new("default", b"fixture profile bytes".to_vec()),
        ))
        .expect("put fixture profile-sync object")
    else {
        panic!("put object returned unexpected response");
    };
    let ProfileSyncResponse::RetainObject { retained: true, .. } = first
        .profile_sync(ProfileSyncRequest::RetainObject(
            ProfileSyncObjectRequest::new("default", object_id.as_str()),
        ))
        .expect("retain fixture profile-sync object")
    else {
        panic!("retain object returned unexpected response");
    };
    first
        .profile_sync(ProfileSyncRequest::PublishRoot(ProfileSyncRootUpdate::new(
            "default",
            "settings/latest",
            object_id.as_str(),
        )))
        .expect("publish fixture profile-sync root");

    let ProfileSyncResponse::Root {
        object_id: Some(resolved_object_id),
        ..
    } = second
        .profile_sync(ProfileSyncRequest::ResolveRoot(
            ProfileSyncRootRequest::new("default", "settings/latest"),
        ))
        .expect("resolve fixture profile-sync root from second daemon")
    else {
        panic!("resolve root returned unexpected response");
    };
    let ProfileSyncResponse::GetEncryptedObject { bytes, .. } = second
        .profile_sync(ProfileSyncRequest::GetEncryptedObject(
            ProfileSyncObjectRequest::new("default", resolved_object_id.as_str()),
        ))
        .expect("fetch fixture profile-sync object from second daemon")
    else {
        panic!("get object returned unexpected response");
    };
    assert_eq!(resolved_object_id, object_id);
    assert_eq!(bytes, b"fixture profile bytes".to_vec());

    let external_error = second
        .fetch_http(HttpFetchRequest::default_profile("https://example.com/"))
        .expect_err("per-device fixture registry must reject external HTTP");
    assert!(matches!(
        external_error,
        BroadwebdError::UnsupportedRequest(message)
            if message.contains("cannot fetch external URL")
    ));

    let _ = std::fs::remove_dir_all(first_state_root);
    let _ = std::fs::remove_dir_all(second_state_root);
}
