use cadence_updater::{ReleaseFeed, USER_AGENT, UpdateError, Updater, Version, https_only_client};
use serde_json::json;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::support::{
    CHECKSUMS, LATEST_PATH, NEW_ASSET, NEW_TAG, feed_of, mount_latest, release_json, updater,
};

fn latest_is(server: &MockServer, tag: &str, assets: &[(&str, usize)]) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(release_json(server, tag, assets))
}

#[tokio::test]
async fn given_a_newer_release_when_checking_then_it_is_offered() {
    let server = MockServer::start().await;
    let assets = [(NEW_ASSET, 7), (CHECKSUMS, 90)];
    mount_latest(&server, latest_is(&server, NEW_TAG, &assets)).await;

    let offer = updater(&server, None).check().await.unwrap().unwrap();

    assert_eq!(offer.version, Version::new(2, 0, 0));
    assert_eq!(offer.page_url, "https://releases.example/v2.0.0");
    assert_eq!(offer.asset.name, NEW_ASSET);
    assert_eq!(offer.asset.size, 7);
    assert_eq!(offer.checksums.name, CHECKSUMS);
}

#[tokio::test]
async fn given_the_installed_version_is_the_latest_when_checking_then_nothing_is_offered() {
    let server = MockServer::start().await;
    let assets = [("Cadence-1.0.0-x86_64.AppImage", 7), (CHECKSUMS, 90)];
    mount_latest(&server, latest_is(&server, "v1.0.0", &assets)).await;

    assert_eq!(updater(&server, None).check().await.unwrap(), None);
}

#[tokio::test]
async fn given_an_older_release_when_checking_then_nothing_is_offered() {
    let server = MockServer::start().await;
    let assets = [("Cadence-0.9.0-x86_64.AppImage", 7), (CHECKSUMS, 90)];
    mount_latest(&server, latest_is(&server, "v0.9.0", &assets)).await;

    assert_eq!(updater(&server, None).check().await.unwrap(), None);
}

#[tokio::test]
async fn given_no_release_was_published_when_checking_then_there_is_no_update_and_no_error() {
    let server = MockServer::start().await;
    let not_found = ResponseTemplate::new(404).set_body_json(json!({ "message": "Not Found" }));
    mount_latest(&server, not_found).await;

    assert_eq!(updater(&server, None).check().await.unwrap(), None);
}

#[tokio::test]
async fn given_the_rate_limit_is_exhausted_when_checking_then_it_is_reported_as_such() {
    let server = MockServer::start().await;
    let limited = ResponseTemplate::new(403)
        .insert_header("x-ratelimit-remaining", "0")
        .set_body_json(json!({ "message": "API rate limit exceeded" }));
    mount_latest(&server, limited).await;

    let error = updater(&server, None).check().await.unwrap_err();

    assert!(matches!(error, UpdateError::RateLimited), "{error}");
}

#[tokio::test]
async fn given_too_many_requests_when_checking_then_it_is_reported_as_rate_limited() {
    let server = MockServer::start().await;
    mount_latest(&server, ResponseTemplate::new(429)).await;

    let error = updater(&server, None).check().await.unwrap_err();

    assert!(matches!(error, UpdateError::RateLimited), "{error}");
}

#[tokio::test]
async fn given_a_server_error_when_checking_then_its_status_is_reported() {
    let server = MockServer::start().await;
    mount_latest(&server, ResponseTemplate::new(503)).await;

    let error = updater(&server, None).check().await.unwrap_err();

    assert!(
        matches!(error, UpdateError::Status(status) if status == 503),
        "{error}"
    );
}

#[tokio::test]
async fn given_no_asset_for_this_platform_when_checking_then_nothing_is_offered() {
    let server = MockServer::start().await;
    let assets = [("Cadence-2.0.0-macos-arm64.zip", 7), (CHECKSUMS, 90)];
    mount_latest(&server, latest_is(&server, NEW_TAG, &assets)).await;

    assert_eq!(updater(&server, None).check().await.unwrap(), None);
}

#[tokio::test]
async fn given_a_release_without_checksums_when_checking_then_nothing_is_offered() {
    let server = MockServer::start().await;
    mount_latest(&server, latest_is(&server, NEW_TAG, &[(NEW_ASSET, 7)])).await;

    assert_eq!(updater(&server, None).check().await.unwrap(), None);
}

#[tokio::test]
async fn given_a_description_that_is_not_json_when_checking_then_it_is_a_decode_error() {
    let server = MockServer::start().await;
    mount_latest(
        &server,
        ResponseTemplate::new(200).set_body_string("<html>"),
    )
    .await;

    let error = updater(&server, None).check().await.unwrap_err();

    assert!(matches!(error, UpdateError::Decode(_)), "{error}");
}

#[tokio::test]
async fn given_an_endless_description_when_checking_then_it_is_refused_as_too_large() {
    let server = MockServer::start().await;
    let endless = "x".repeat(2 * 1024 * 1024);
    mount_latest(&server, ResponseTemplate::new(200).set_body_string(endless)).await;

    let error = updater(&server, None).check().await.unwrap_err();

    assert!(matches!(error, UpdateError::TooLarge(_)), "{error}");
}

#[tokio::test]
async fn given_the_github_client_when_checking_then_it_identifies_itself_without_credentials() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(LATEST_PATH))
        .and(header("accept", "application/vnd.github+json"))
        .respond_with(ResponseTemplate::new(404))
        .expect(1)
        .mount(&server)
        .await;
    let client = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .build()
        .unwrap();
    let feed = ReleaseFeed::new(client, format!("{}{LATEST_PATH}", server.uri()));

    feed.latest().await.unwrap();

    let requests = server.received_requests().await.unwrap();
    let request = requests.first().unwrap();
    assert_eq!(request.headers.get("user-agent").unwrap(), USER_AGENT);
    assert!(request.headers.get("authorization").is_none());
}

#[tokio::test]
async fn given_a_plain_http_address_when_using_the_https_only_client_then_nothing_is_requested() {
    let server = MockServer::start().await;
    let feed = ReleaseFeed::new(
        https_only_client().unwrap(),
        format!("{}{LATEST_PATH}", server.uri()),
    );

    let error = feed.latest().await.unwrap_err();

    assert!(matches!(error, UpdateError::Network(_)), "{error}");
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn given_a_binary_outside_any_package_when_detecting_then_it_cannot_install_itself() {
    let updater = Updater::for_this_install().await.unwrap();

    assert!(!updater.can_install());
}

#[tokio::test]
async fn given_the_feed_alone_when_reading_the_latest_release_then_its_assets_are_listed() {
    let server = MockServer::start().await;
    mount_latest(&server, latest_is(&server, NEW_TAG, &[(CHECKSUMS, 90)])).await;

    let release = feed_of(&server).latest().await.unwrap().unwrap();

    assert_eq!(release.tag_name, NEW_TAG);
    assert_eq!(release.assets.len(), 1);
}
