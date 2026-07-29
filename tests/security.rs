use axum::{
    Router,
    body::Body,
    http::{Method, Request, StatusCode, header},
};
use http_body_util::BodyExt;
use printlatch::{
    api::{AppState, router},
    auth,
    config::AppConfig,
    db::Database,
    worker::QueueSignal,
};
use serde_json::Value;
use tempfile::TempDir;
use tower::ServiceExt;

const HOST: &str = "127.0.0.1:32191";
const BOUNDARY: &str = "printlatch-test-boundary";
const AGENT_SESSION: &str = "test-session-value-000000000000000000000000";

struct Harness {
    temp: TempDir,
    app: Router,
    db: Database,
    token: String,
}

impl Harness {
    fn new() -> Self {
        let temp = tempfile::tempdir().expect("temporary directory");
        let config =
            AppConfig::resolve(Some(temp.path().to_path_buf()), None).expect("test config");
        config.ensure_directories().expect("test directories");
        let db = Database::open(config.database_path()).expect("test database");
        let token = auth::issue_local_token(&db, "integration test", 1).expect("local test token");
        let app = router(AppState {
            config,
            db: db.clone(),
            queue: QueueSignal::new(),
            instance_session: AGENT_SESSION.to_owned(),
        });
        Self {
            temp,
            app,
            db,
            token: token.token,
        }
    }

    fn authenticated(&self, method: Method, uri: &str, body: Body) -> Request<Body> {
        Request::builder()
            .method(method)
            .uri(uri)
            .header(header::HOST, HOST)
            .header(header::AUTHORIZATION, format!("Bearer {}", self.token))
            .body(body)
            .expect("request")
    }
}

#[tokio::test]
async fn pairing_is_origin_bound_one_time_and_replay_safe() {
    let harness = Harness::new();
    let grant = auth::new_pairing_grant(&harness.db, "https://app.example", "browser")
        .expect("pairing grant");

    let wrong = json_request(
        Method::POST,
        "/v1/pair",
        &serde_json::json!({ "code": grant.code }),
        Some("https://evil.example"),
    );
    let wrong_response = harness
        .app
        .clone()
        .oneshot(wrong)
        .await
        .expect("wrong-origin response");
    assert_eq!(wrong_response.status(), StatusCode::UNAUTHORIZED);

    let valid = json_request(
        Method::POST,
        "/v1/pair",
        &serde_json::json!({ "code": grant.code }),
        Some("https://app.example"),
    );
    let valid_response = harness
        .app
        .clone()
        .oneshot(valid)
        .await
        .expect("valid pair response");
    assert_eq!(valid_response.status(), StatusCode::OK);
    let valid_json = response_json(valid_response).await;
    let browser_token = valid_json["token"].as_str().expect("issued browser token");

    let replay = json_request(
        Method::POST,
        "/v1/pair",
        &serde_json::json!({ "code": grant.code }),
        Some("https://app.example"),
    );
    let replay_response = harness
        .app
        .clone()
        .oneshot(replay)
        .await
        .expect("replay response");
    assert_eq!(replay_response.status(), StatusCode::UNAUTHORIZED);

    let accepted = Request::builder()
        .method(Method::GET)
        .uri("/v1/printers")
        .header(header::HOST, HOST)
        .header(header::ORIGIN, "https://app.example")
        .header(header::AUTHORIZATION, format!("Bearer {browser_token}"))
        .body(Body::empty())
        .expect("origin request");
    assert_eq!(
        harness
            .app
            .clone()
            .oneshot(accepted)
            .await
            .expect("accepted response")
            .status(),
        StatusCode::OK
    );

    let stolen = Request::builder()
        .method(Method::GET)
        .uri("/v1/printers")
        .header(header::HOST, HOST)
        .header(header::ORIGIN, "https://evil.example")
        .header(header::AUTHORIZATION, format!("Bearer {browser_token}"))
        .body(Body::empty())
        .expect("stolen-token request");
    assert_eq!(
        harness
            .app
            .clone()
            .oneshot(stolen)
            .await
            .expect("rejected response")
            .status(),
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn local_tokens_cannot_be_replayed_from_a_browser_origin() {
    let harness = Harness::new();
    let request = Request::builder()
        .method(Method::GET)
        .uri("/v1/printers")
        .header(header::HOST, HOST)
        .header(header::ORIGIN, "https://evil.example")
        .header(header::AUTHORIZATION, format!("Bearer {}", harness.token))
        .body(Body::empty())
        .expect("browser request");
    let response = harness.app.oneshot(request).await.expect("response");
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn local_dashboard_get_requires_browser_proven_same_origin() {
    let harness = Harness::new();
    let origin = format!("http://{HOST}");
    let grant = auth::new_pairing_grant(&harness.db, &origin, "dashboard").expect("pairing grant");
    let pair_response = harness
        .app
        .clone()
        .oneshot(json_request(
            Method::POST,
            "/v1/pair",
            &serde_json::json!({ "code": grant.code }),
            Some(&origin),
        ))
        .await
        .expect("pair response");
    let token = response_json(pair_response)
        .await
        .get("token")
        .and_then(Value::as_str)
        .expect("browser token")
        .to_owned();

    let accepted = Request::builder()
        .method(Method::GET)
        .uri("/v1/printers")
        .header(header::HOST, HOST)
        .header("sec-fetch-site", "same-origin")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .expect("same-origin request");
    assert_eq!(
        harness
            .app
            .clone()
            .oneshot(accepted)
            .await
            .expect("accepted response")
            .status(),
        StatusCode::OK
    );

    let missing_browser_proof = Request::builder()
        .method(Method::GET)
        .uri("/v1/printers")
        .header(header::HOST, HOST)
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .expect("request without browser proof");
    assert_eq!(
        harness
            .app
            .oneshot(missing_browser_proof)
            .await
            .expect("rejected response")
            .status(),
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn bundled_test_pdf_requires_authentication() {
    let harness = Harness::new();
    let unauthenticated = Request::builder()
        .method(Method::GET)
        .uri("/app/test-page.pdf")
        .header(header::HOST, HOST)
        .body(Body::empty())
        .expect("unauthenticated request");
    assert_eq!(
        harness
            .app
            .clone()
            .oneshot(unauthenticated)
            .await
            .expect("response")
            .status(),
        StatusCode::UNAUTHORIZED
    );

    let authenticated = harness.authenticated(Method::GET, "/app/test-page.pdf", Body::empty());
    let response = harness
        .app
        .oneshot(authenticated)
        .await
        .expect("authenticated response");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE),
        Some(&header::HeaderValue::from_static("application/pdf"))
    );
    assert_eq!(
        response.headers().get(header::CACHE_CONTROL),
        Some(&header::HeaderValue::from_static("no-store, private"))
    );
}

#[tokio::test]
async fn dashboard_repair_rotates_token_and_preserves_job_history() {
    let harness = Harness::new();
    let origin = format!("http://{HOST}");
    let first = pair_dashboard(&harness, &origin).await;
    let first_token = first["token"].as_str().expect("first token");
    let first_client = first["client_id"].as_str().expect("first client");

    let body = multipart_body(&[
        text_part("mode", "preview"),
        file_part("history.pdf", "application/pdf", &minimal_pdf()),
    ]);
    let mut create = Request::builder()
        .method(Method::POST)
        .uri("/v1/jobs")
        .header(header::HOST, HOST)
        .header(header::ORIGIN, &origin)
        .header(header::AUTHORIZATION, format!("Bearer {first_token}"))
        .body(Body::from(body))
        .expect("create request");
    create.headers_mut().insert(
        header::CONTENT_TYPE,
        format!("multipart/form-data; boundary={BOUNDARY}")
            .parse()
            .expect("multipart content type"),
    );
    assert_eq!(
        harness
            .app
            .clone()
            .oneshot(create)
            .await
            .expect("create response")
            .status(),
        StatusCode::ACCEPTED
    );

    let second = pair_dashboard(&harness, &origin).await;
    let second_token = second["token"].as_str().expect("second token");
    assert_eq!(
        second["client_id"].as_str().expect("second client"),
        first_client
    );
    assert_ne!(second_token, first_token);

    let old_token = Request::builder()
        .method(Method::GET)
        .uri("/v1/jobs")
        .header(header::HOST, HOST)
        .header(header::ORIGIN, &origin)
        .header(header::AUTHORIZATION, format!("Bearer {first_token}"))
        .body(Body::empty())
        .expect("old-token request");
    assert_eq!(
        harness
            .app
            .clone()
            .oneshot(old_token)
            .await
            .expect("old-token response")
            .status(),
        StatusCode::UNAUTHORIZED
    );

    let new_token = Request::builder()
        .method(Method::GET)
        .uri("/v1/jobs")
        .header(header::HOST, HOST)
        .header(header::ORIGIN, &origin)
        .header(header::AUTHORIZATION, format!("Bearer {second_token}"))
        .body(Body::empty())
        .expect("new-token request");
    let jobs = response_json(
        harness
            .app
            .oneshot(new_token)
            .await
            .expect("new-token response"),
    )
    .await;
    assert_eq!(jobs["jobs"].as_array().expect("jobs").len(), 1);
}

#[tokio::test]
async fn generic_browser_grants_keep_clients_and_job_history_separate() {
    let harness = Harness::new();
    let origin = "https://app.example";
    let first = pair_browser(&harness, origin, "Browser app").await;
    let first_token = first["token"].as_str().expect("first token");

    let body = multipart_body(&[
        text_part("mode", "preview"),
        file_part("private-history.pdf", "application/pdf", &minimal_pdf()),
    ]);
    let mut create = Request::builder()
        .method(Method::POST)
        .uri("/v1/jobs")
        .header(header::HOST, HOST)
        .header(header::ORIGIN, origin)
        .header(header::AUTHORIZATION, format!("Bearer {first_token}"))
        .body(Body::from(body))
        .expect("create request");
    create.headers_mut().insert(
        header::CONTENT_TYPE,
        format!("multipart/form-data; boundary={BOUNDARY}")
            .parse()
            .expect("multipart content type"),
    );
    assert_eq!(
        harness
            .app
            .clone()
            .oneshot(create)
            .await
            .expect("create response")
            .status(),
        StatusCode::ACCEPTED
    );

    let second = pair_browser(&harness, origin, "Browser app").await;
    assert_ne!(first["client_id"], second["client_id"]);

    let second_jobs = Request::builder()
        .method(Method::GET)
        .uri("/v1/jobs")
        .header(header::HOST, HOST)
        .header(header::ORIGIN, origin)
        .header(
            header::AUTHORIZATION,
            format!("Bearer {}", second["token"].as_str().expect("second token")),
        )
        .body(Body::empty())
        .expect("second-client request");
    let jobs = response_json(
        harness
            .app
            .clone()
            .oneshot(second_jobs)
            .await
            .expect("second-client response"),
    )
    .await;
    assert!(jobs["jobs"].as_array().expect("jobs").is_empty());

    let first_still_valid = Request::builder()
        .method(Method::GET)
        .uri("/v1/jobs")
        .header(header::HOST, HOST)
        .header(header::ORIGIN, origin)
        .header(header::AUTHORIZATION, format!("Bearer {first_token}"))
        .body(Body::empty())
        .expect("first-client request");
    assert_eq!(
        harness
            .app
            .oneshot(first_still_valid)
            .await
            .expect("first-client response")
            .status(),
        StatusCode::OK
    );
}

#[tokio::test]
async fn dashboard_grant_is_bound_to_the_running_agent_session() {
    let harness = Harness::new();
    let origin = format!("http://{HOST}");
    let stale_session = "stale-session-value-00000000000000000000000";
    assert!(auth::valid_agent_session(stale_session));
    let grant = auth::new_dashboard_pairing_grant(
        &harness.db,
        &origin,
        "PrintLatch dashboard",
        stale_session,
    )
    .expect("session-bound grant");
    let response = harness
        .app
        .oneshot(json_request(
            Method::POST,
            "/v1/pair",
            &serde_json::json!({ "code": grant.code }),
            Some(&origin),
        ))
        .await
        .expect("pair response");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn rejects_dns_rebinding_and_websocket_upgrade() {
    let harness = Harness::new();
    let rebound = Request::builder()
        .method(Method::GET)
        .uri("/health")
        .header(header::HOST, "print.attacker.example:32191")
        .body(Body::empty())
        .expect("rebound request");
    assert_eq!(
        harness
            .app
            .clone()
            .oneshot(rebound)
            .await
            .expect("response")
            .status(),
        StatusCode::FORBIDDEN
    );

    let websocket = Request::builder()
        .method(Method::GET)
        .uri("/v1/jobs")
        .header(header::HOST, HOST)
        .header(header::UPGRADE, "websocket")
        .body(Body::empty())
        .expect("websocket request");
    assert_eq!(
        harness
            .app
            .oneshot(websocket)
            .await
            .expect("response")
            .status(),
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn rejects_ssrf_fields_wrong_mime_and_zip_payloads() {
    let harness = Harness::new();
    let with_url = multipart_body(&[
        text_part("mode", "preview"),
        text_part("sourceUrl", "http://169.254.169.254/latest/meta-data"),
        file_part("invoice.pdf", "application/pdf", &minimal_pdf()),
    ]);
    let request = multipart_request(&harness, with_url);
    let response = harness
        .app
        .clone()
        .oneshot(request)
        .await
        .expect("SSRF response");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let wrong_mime = multipart_body(&[
        text_part("mode", "preview"),
        file_part("invoice.pdf", "application/octet-stream", &minimal_pdf()),
    ]);
    let response = harness
        .app
        .clone()
        .oneshot(multipart_request(&harness, wrong_mime))
        .await
        .expect("MIME response");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let zip = multipart_body(&[
        text_part("mode", "preview"),
        file_part("invoice.pdf", "application/pdf", b"PK\x03\x04zip data"),
    ]);
    let response = harness
        .app
        .clone()
        .oneshot(multipart_request(&harness, zip))
        .await
        .expect("ZIP response");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn ignores_traversal_filename_and_isolates_jobs_between_clients() {
    let harness = Harness::new();
    let body = multipart_body(&[
        text_part("mode", "preview"),
        file_part("../../outside.pdf", "application/pdf", &minimal_pdf()),
    ]);
    let response = harness
        .app
        .clone()
        .oneshot(multipart_request(&harness, body))
        .await
        .expect("create response");
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let json = response_json(response).await;
    let job_id = json["job"]["id"].as_str().expect("job id");
    assert!(!harness.temp.path().join("outside.pdf").exists());
    assert!(
        harness
            .temp
            .path()
            .join("jobs")
            .join(format!("{job_id}.pdf"))
            .exists()
    );

    let other = auth::issue_local_token(&harness.db, "other client", 1).expect("other token");
    let request = Request::builder()
        .method(Method::GET)
        .uri(format!("/v1/jobs/{job_id}"))
        .header(header::HOST, HOST)
        .header(header::AUTHORIZATION, format!("Bearer {}", other.token))
        .body(Body::empty())
        .expect("other-client request");
    let response = harness
        .app
        .oneshot(request)
        .await
        .expect("isolation response");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn requires_authentication_and_rejects_oversized_jobs() {
    let harness = Harness::new();
    let unauthenticated = Request::builder()
        .method(Method::GET)
        .uri("/v1/jobs")
        .header(header::HOST, HOST)
        .body(Body::empty())
        .expect("unauthenticated request");
    assert_eq!(
        harness
            .app
            .clone()
            .oneshot(unauthenticated)
            .await
            .expect("response")
            .status(),
        StatusCode::UNAUTHORIZED
    );

    let oversized = vec![b'x'; printlatch::MAX_JOB_BYTES + 128 * 1024];
    let body = multipart_body(&[
        text_part("mode", "preview"),
        file_part("oversized.pdf", "application/pdf", &oversized),
    ]);
    let request = multipart_request(&harness, body);
    let response = harness
        .app
        .oneshot(request)
        .await
        .expect("oversize response");
    let status = response.status();
    let response_body = response
        .into_body()
        .collect()
        .await
        .expect("oversize body")
        .to_bytes();
    assert_eq!(
        status,
        StatusCode::PAYLOAD_TOO_LARGE,
        "{}",
        String::from_utf8_lossy(&response_body)
    );
}

fn json_request(method: Method, uri: &str, value: &Value, origin: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::HOST, HOST)
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(origin) = origin {
        builder = builder.header(header::ORIGIN, origin);
    }
    builder
        .body(Body::from(value.to_string()))
        .expect("JSON request")
}

async fn pair_browser(harness: &Harness, origin: &str, name: &str) -> Value {
    let grant = auth::new_pairing_grant(&harness.db, origin, name).expect("pairing grant");
    let response = harness
        .app
        .clone()
        .oneshot(json_request(
            Method::POST,
            "/v1/pair",
            &serde_json::json!({ "code": grant.code }),
            Some(origin),
        ))
        .await
        .expect("pair response");
    assert_eq!(response.status(), StatusCode::OK);
    response_json(response).await
}

async fn pair_dashboard(harness: &Harness, origin: &str) -> Value {
    let grant = auth::new_dashboard_pairing_grant(
        &harness.db,
        origin,
        "PrintLatch dashboard",
        AGENT_SESSION,
    )
    .expect("dashboard pairing grant");
    let response = harness
        .app
        .clone()
        .oneshot(json_request(
            Method::POST,
            "/v1/pair",
            &serde_json::json!({ "code": grant.code }),
            Some(origin),
        ))
        .await
        .expect("dashboard pair response");
    assert_eq!(response.status(), StatusCode::OK);
    response_json(response).await
}

fn multipart_request(harness: &Harness, body: Vec<u8>) -> Request<Body> {
    let mut request = harness.authenticated(Method::POST, "/v1/jobs", Body::from(body));
    request.headers_mut().insert(
        header::CONTENT_TYPE,
        format!("multipart/form-data; boundary={BOUNDARY}")
            .parse()
            .expect("multipart content type"),
    );
    request
}

fn text_part(name: &str, value: &str) -> Vec<u8> {
    format!("Content-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}").into_bytes()
}

fn file_part(filename: &str, mime: &str, bytes: &[u8]) -> Vec<u8> {
    let mut part = format!(
        "Content-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\nContent-Type: {mime}\r\n\r\n"
    )
    .into_bytes();
    part.extend_from_slice(bytes);
    part
}

fn multipart_body(parts: &[Vec<u8>]) -> Vec<u8> {
    let mut body = Vec::new();
    for part in parts {
        body.extend_from_slice(format!("--{BOUNDARY}\r\n").as_bytes());
        body.extend_from_slice(part);
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{BOUNDARY}--\r\n").as_bytes());
    body
}

async fn response_json(response: axum::response::Response) -> Value {
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("response body")
        .to_bytes();
    serde_json::from_slice(&bytes).expect("JSON response")
}

fn minimal_pdf() -> Vec<u8> {
    br"%PDF-1.4
1 0 obj
<< /Type /Catalog /Pages 2 0 R >>
endobj
2 0 obj
<< /Type /Pages /Kids [3 0 R] /Count 1 >>
endobj
3 0 obj
<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>
endobj
xref
0 4
0000000000 65535 f 
0000000009 00000 n 
0000000058 00000 n 
0000000115 00000 n 
trailer
<< /Size 4 /Root 1 0 R >>
startxref
186
%%EOF
"
    .to_vec()
}
