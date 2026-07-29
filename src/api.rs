use std::path::PathBuf;

use axum::{
    Json, Router,
    body::Body,
    extract::{DefaultBodyLimit, Multipart, Path, Query, State},
    http::{
        HeaderMap, HeaderValue, Method, Request, StatusCode,
        header::{self, HeaderName},
    },
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, options, post},
};
use serde::{Deserialize, Serialize};
use tokio::fs;
use uuid::Uuid;

use crate::{
    MAX_JOB_BYTES, VERSION,
    auth::{self, IssuedToken},
    config::AppConfig,
    db::{Database, Job, NewJob},
    error::AppError,
    pdf_guard,
    printers::{self, PrinterInfo},
    worker::QueueSignal,
};

const ACCESS_CONTROL_ALLOW_PRIVATE_NETWORK: HeaderName =
    HeaderName::from_static("access-control-allow-private-network");
const ACCESS_CONTROL_REQUEST_PRIVATE_NETWORK: HeaderName =
    HeaderName::from_static("access-control-request-private-network");

#[derive(Clone)]
pub struct AppState {
    pub config: AppConfig,
    pub db: Database,
    pub queue: QueueSignal,
}

#[derive(Serialize)]
struct Health {
    status: &'static str,
    product: &'static str,
    version: &'static str,
    bind: &'static str,
    telemetry: bool,
}

#[derive(Deserialize)]
struct PairRequest {
    code: String,
}

#[derive(Serialize)]
struct PrintersResponse {
    printers: Vec<PrinterInfo>,
}

#[derive(Serialize)]
struct JobResponse {
    job: Job,
}

#[derive(Serialize)]
struct JobsResponse {
    jobs: Vec<Job>,
}

#[derive(Deserialize)]
struct ListQuery {
    limit: Option<u16>,
}

struct ParsedJob {
    bytes: Vec<u8>,
    mime: String,
    mode: String,
    printer_id: String,
    copies: u8,
}

pub fn router(state: AppState) -> Router {
    let max_body = MAX_JOB_BYTES + 64 * 1024;
    Router::new()
        .route("/health", get(health))
        .route("/v1/pair", post(pair))
        .route("/v1/printers", get(list_printers))
        .route("/v1/jobs", post(create_job).get(list_jobs))
        .route("/v1/jobs/{id}", get(get_job))
        .route("/v1/jobs/{id}/document", get(get_document))
        .route("/v1/jobs/{id}/cancel", post(cancel_job))
        .route("/v1/jobs/{id}/retry", post(retry_job))
        .route("/{*path}", options(preflight))
        .layer(DefaultBodyLimit::max(max_body))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            security_headers,
        ))
        .with_state(state)
}

async fn health() -> Json<Health> {
    Json(Health {
        status: "ok",
        product: "PrintLatch",
        version: VERSION,
        bind: "loopback-only",
        telemetry: false,
    })
}

async fn pair(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<PairRequest>,
) -> Result<Json<IssuedToken>, AppError> {
    let origin = request_origin(&headers)?;
    let token = auth::consume_pairing_code(&state.db, &request.code, origin)?;
    Ok(Json(token))
}

async fn list_printers(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<PrintersResponse>, AppError> {
    auth::authenticate(&state.db, &headers)?;
    let printers = tokio::task::spawn_blocking(printers::list_printers)
        .await
        .map_err(AppError::internal)?
        .map_err(AppError::internal)?;
    Ok(Json(PrintersResponse { printers }))
}

async fn create_job(
    State(state): State<AppState>,
    headers: HeaderMap,
    multipart: Multipart,
) -> Result<(StatusCode, Json<JobResponse>), AppError> {
    let client = auth::authenticate(&state.db, &headers)?;
    let parsed = parse_job(multipart).await?;
    if parsed.mode == "print" {
        let printer_id = parsed.printer_id.clone();
        let available = tokio::task::spawn_blocking(move || printers::printer_exists(&printer_id))
            .await
            .map_err(AppError::internal)?
            .map_err(AppError::internal)?;
        if !available {
            return Err(AppError::BadRequest(
                "printer_id is not currently available".to_owned(),
            ));
        }
    }
    let report = pdf_guard::validate_pdf(&parsed.bytes, &parsed.mime)
        .map_err(|error| AppError::BadRequest(error.to_string()))?;
    let id = Uuid::new_v4().to_string();
    let path = state.config.jobs_dir().join(format!("{id}.pdf"));
    write_job_file(&path, &parsed.bytes).await?;
    let state_name = if parsed.mode == "preview" {
        "preview_ready"
    } else {
        "queued"
    };
    let stored_path = path.to_string_lossy().into_owned();
    let new_job = NewJob {
        id: &id,
        client_id: &client.id,
        printer_id: if parsed.mode == "preview" {
            "capture:pdf"
        } else {
            &parsed.printer_id
        },
        state: state_name,
        mode: &parsed.mode,
        copies: parsed.copies,
        page_count: report.page_count,
        byte_count: report.byte_count,
        sha256: &report.sha256,
        file_path: &stored_path,
    };
    if let Err(error) = state.db.insert_job(&new_job) {
        let _ = fs::remove_file(&path).await;
        return Err(AppError::internal(error));
    }
    if parsed.mode == "print" {
        state.queue.notify();
    }
    let job = state
        .db
        .get_job_for_client(&id, &client.id)
        .map_err(AppError::internal)?
        .ok_or_else(|| AppError::internal("created job could not be reloaded"))?;
    Ok((StatusCode::ACCEPTED, Json(JobResponse { job })))
}

async fn list_jobs(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ListQuery>,
) -> Result<Json<JobsResponse>, AppError> {
    let client = auth::authenticate(&state.db, &headers)?;
    let limit = query.limit.unwrap_or(25).clamp(1, 100);
    let jobs = state
        .db
        .list_jobs_for_client(&client.id, limit)
        .map_err(AppError::internal)?;
    Ok(Json(JobsResponse { jobs }))
}

async fn get_job(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<JobResponse>, AppError> {
    let client = auth::authenticate(&state.db, &headers)?;
    let job = state
        .db
        .get_job_for_client(&id, &client.id)
        .map_err(AppError::internal)?
        .ok_or_else(|| AppError::NotFound("job not found".to_owned()))?;
    Ok(Json(JobResponse { job }))
}

async fn get_document(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Response, AppError> {
    let client = auth::authenticate(&state.db, &headers)?;
    let (job, path) = state
        .db
        .get_job_file_for_client(&id, &client.id)
        .map_err(AppError::internal)?
        .ok_or_else(|| AppError::NotFound("job not found".to_owned()))?;
    if !matches!(
        job.state.as_str(),
        "preview_ready" | "queued" | "printing" | "succeeded" | "failed" | "unknown"
    ) {
        return Err(AppError::Conflict(
            "document is not available for this job state".to_owned(),
        ));
    }
    let bytes = fs::read(&path).await.map_err(AppError::internal)?;
    let mut response = Response::new(Body::from(bytes));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/pdf"),
    );
    response.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("inline; filename=\"printlatch-{id}.pdf\""))
            .map_err(AppError::internal)?,
    );
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-store, private"),
    );
    response.headers_mut().insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    Ok(response)
}

async fn cancel_job(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<JobResponse>, AppError> {
    let client = auth::authenticate(&state.db, &headers)?;
    if !state
        .db
        .cancel_job(&id, &client.id)
        .map_err(AppError::internal)?
    {
        return Err(AppError::Conflict(
            "only queued jobs can be canceled".to_owned(),
        ));
    }
    let job = state
        .db
        .get_job_for_client(&id, &client.id)
        .map_err(AppError::internal)?
        .ok_or_else(|| AppError::NotFound("job not found".to_owned()))?;
    Ok(Json(JobResponse { job }))
}

async fn retry_job(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<JobResponse>, AppError> {
    let client = auth::authenticate(&state.db, &headers)?;
    if !state
        .db
        .retry_job(&id, &client.id)
        .map_err(AppError::internal)?
    {
        return Err(AppError::Conflict(
            "only failed or interrupted jobs with fewer than three attempts can be retried"
                .to_owned(),
        ));
    }
    state.queue.notify();
    let job = state
        .db
        .get_job_for_client(&id, &client.id)
        .map_err(AppError::internal)?
        .ok_or_else(|| AppError::NotFound("job not found".to_owned()))?;
    Ok(Json(JobResponse { job }))
}

async fn preflight() -> StatusCode {
    StatusCode::NO_CONTENT
}

async fn security_headers(
    State(state): State<AppState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let headers = request.headers();
    if !valid_host(headers, state.config.port) {
        return AppError::Forbidden.into_response();
    }
    if headers
        .get(header::UPGRADE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.eq_ignore_ascii_case("websocket"))
    {
        return AppError::BadRequest("WebSocket upgrades are not supported".to_owned())
            .into_response();
    }
    let origin = headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| auth::normalize_origin(value).ok());
    let private_network_requested = headers
        .get(&ACCESS_CONTROL_REQUEST_PRIVATE_NETWORK)
        .is_some_and(|value| value == "true");
    let method = request.method().clone();
    let mut response = next.run(request).await;
    apply_common_headers(response.headers_mut());
    if let Some(origin) = origin {
        if let Ok(origin_header) = HeaderValue::from_str(&origin) {
            response
                .headers_mut()
                .insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, origin_header);
            response
                .headers_mut()
                .insert(header::VARY, HeaderValue::from_static("Origin"));
        }
        if method == Method::OPTIONS {
            response.headers_mut().insert(
                header::ACCESS_CONTROL_ALLOW_METHODS,
                HeaderValue::from_static("GET, POST, OPTIONS"),
            );
            response.headers_mut().insert(
                header::ACCESS_CONTROL_ALLOW_HEADERS,
                HeaderValue::from_static("Authorization, Content-Type"),
            );
            response.headers_mut().insert(
                header::ACCESS_CONTROL_MAX_AGE,
                HeaderValue::from_static("600"),
            );
            if private_network_requested {
                response.headers_mut().insert(
                    ACCESS_CONTROL_ALLOW_PRIVATE_NETWORK,
                    HeaderValue::from_static("true"),
                );
            }
        }
    }
    response
}

fn apply_common_headers(headers: &mut HeaderMap) {
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-store, private"),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    headers.insert(
        HeaderName::from_static("x-frame-options"),
        HeaderValue::from_static("DENY"),
    );
}

fn valid_host(headers: &HeaderMap, port: u16) -> bool {
    let Some(host) = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
    else {
        return false;
    };
    host.eq_ignore_ascii_case(&format!("127.0.0.1:{port}"))
        || host.eq_ignore_ascii_case(&format!("localhost:{port}"))
        || host.eq_ignore_ascii_case(&format!("[::1]:{port}"))
}

fn request_origin(headers: &HeaderMap) -> Result<&str, AppError> {
    headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        .ok_or(AppError::Forbidden)
}

async fn parse_job(mut multipart: Multipart) -> Result<ParsedJob, AppError> {
    let mut bytes = None;
    let mut mime = None;
    let mut mode = None;
    let mut printer_id = None;
    let mut copies = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|error| multipart_error(&error))?
    {
        let name = field
            .name()
            .ok_or_else(|| AppError::BadRequest("multipart field name is missing".to_owned()))?
            .to_owned();
        match name.as_str() {
            "file" => {
                if bytes.is_some() {
                    return Err(AppError::BadRequest(
                        "exactly one PDF file is required".to_owned(),
                    ));
                }
                mime = field.content_type().map(ToOwned::to_owned);
                let value = field
                    .bytes()
                    .await
                    .map_err(|error| multipart_error(&error))?;
                bytes = Some(value.to_vec());
            }
            "mode" => {
                mode = Some(text_field(field, 16).await?);
            }
            "printer_id" => {
                printer_id = Some(text_field(field, 128).await?);
            }
            "copies" => {
                let raw = text_field(field, 3).await?;
                copies = Some(raw.parse::<u8>().map_err(|_| {
                    AppError::BadRequest("copies must be an integer from 1 to 10".to_owned())
                })?);
            }
            _ => {
                return Err(AppError::BadRequest(format!(
                    "unexpected multipart field: {name}"
                )));
            }
        }
    }
    let mode = mode.ok_or_else(|| AppError::BadRequest("mode is required".to_owned()))?;
    if !matches!(mode.as_str(), "preview" | "print") {
        return Err(AppError::BadRequest(
            "mode must be preview or print".to_owned(),
        ));
    }
    let copies = copies.unwrap_or(1);
    if !(1..=10).contains(&copies) {
        return Err(AppError::BadRequest(
            "copies must be between 1 and 10".to_owned(),
        ));
    }
    let printer_id = printer_id.unwrap_or_else(|| "capture:pdf".to_owned());
    if mode == "print" && printer_id.is_empty() {
        return Err(AppError::BadRequest(
            "printer_id is required for print mode".to_owned(),
        ));
    }
    Ok(ParsedJob {
        bytes: bytes.ok_or_else(|| AppError::BadRequest("file is required".to_owned()))?,
        mime: mime.ok_or_else(|| {
            AppError::BadRequest("file Content-Type must be application/pdf".to_owned())
        })?,
        mode,
        printer_id,
        copies,
    })
}

async fn text_field(
    field: axum::extract::multipart::Field<'_>,
    max: usize,
) -> Result<String, AppError> {
    let value = field
        .text()
        .await
        .map_err(|error| multipart_error(&error))?;
    if value.len() > max {
        return Err(AppError::BadRequest(
            "multipart field is too long".to_owned(),
        ));
    }
    Ok(value)
}

fn multipart_error(error: &axum::extract::multipart::MultipartError) -> AppError {
    if error.status() == StatusCode::PAYLOAD_TOO_LARGE {
        return AppError::PayloadTooLarge("job exceeds the 10 MiB V1 limit".to_owned());
    }
    let message = error.to_string();
    if message.contains("length limit") || message.contains("body too large") {
        AppError::PayloadTooLarge("job exceeds the 10 MiB V1 limit".to_owned())
    } else {
        AppError::BadRequest(format!("invalid multipart body: {message}"))
    }
}

async fn write_job_file(path: &PathBuf, bytes: &[u8]) -> Result<(), AppError> {
    let tmp = path.with_extension("pdf.tmp");
    fs::write(&tmp, bytes).await.map_err(AppError::internal)?;
    fs::rename(&tmp, path).await.map_err(AppError::internal)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_dns_rebinding_hosts() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::HOST,
            HeaderValue::from_static("attacker.example:32191"),
        );
        assert!(!valid_host(&headers, 32_191));
        headers.insert(header::HOST, HeaderValue::from_static("127.0.0.1:32191"));
        assert!(valid_host(&headers, 32_191));
    }
}
