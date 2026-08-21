#![forbid(unsafe_code)]
#![cfg(feature = "test-fixtures")]

use slate_broadwebd::{
    BroadwebDaemon, HttpFetchRequest, HttpHeader, PluginRegistry, ResourceBudget,
    test_fixtures::{InProcessBroadwebNetwork, InternalFixtureHttpResponse},
};

#[test]
fn daemon_fetches_fixture_http_without_loopback_transport() {
    let fixture = InProcessBroadwebNetwork::new().http_response(InternalFixtureHttpResponse {
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
        PluginRegistry::with_default_http(),
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
