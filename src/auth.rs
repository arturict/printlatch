use std::time::Duration;

use anyhow::{Context, Result, bail};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::Utc;
use getrandom::fill;
use http::HeaderMap;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use url::Url;
use uuid::Uuid;

use crate::{db::Database, error::AppError};

const TOKEN_PREFIX: &str = "pl_live_";
const PAIRING_TTL: Duration = Duration::from_secs(5 * 60);
const MAX_CLIENT_DAYS: i64 = 90;

#[derive(Clone, Debug, Serialize)]
pub struct AuthenticatedClient {
    pub id: String,
    pub name: String,
    pub kind: ClientKind,
    pub origin: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ClientKind {
    Browser,
    Local,
}

impl ClientKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Browser => "browser",
            Self::Local => "local",
        }
    }
}

impl TryFrom<&str> for ClientKind {
    type Error = anyhow::Error;

    fn try_from(value: &str) -> Result<Self> {
        match value {
            "browser" => Ok(Self::Browser),
            "local" => Ok(Self::Local),
            _ => bail!("unknown client kind"),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PairingGrant {
    pub code: String,
    pub origin: String,
    pub expires_at: i64,
}

#[derive(Debug, Serialize)]
pub struct IssuedToken {
    pub client_id: String,
    pub token: String,
    pub expires_at: i64,
}

pub fn normalize_origin(value: &str) -> Result<String> {
    let parsed = Url::parse(value).context("origin must be an absolute URL")?;
    if parsed.username() != ""
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
        || parsed.path() != "/"
    {
        bail!("origin must contain only scheme, host, and optional port");
    }
    let host = parsed.host_str().context("origin must include a host")?;
    let is_loopback = matches!(host, "localhost" | "127.0.0.1" | "::1");
    if parsed.scheme() != "https" && !(parsed.scheme() == "http" && is_loopback) {
        bail!("browser origins must use HTTPS, except localhost development origins");
    }
    let mut normalized = format!("{}://{}", parsed.scheme(), host);
    if let Some(port) = parsed.port() {
        normalized.push(':');
        normalized.push_str(&port.to_string());
    }
    Ok(normalized)
}

pub fn new_pairing_grant(db: &Database, origin: &str, name: &str) -> Result<PairingGrant> {
    let origin = normalize_origin(origin)?;
    let code = format_pairing_code(&random_bytes::<16>()?);
    let expires_at = Utc::now().timestamp()
        + i64::try_from(PAIRING_TTL.as_secs()).context("pairing TTL is out of range")?;
    db.insert_pairing_code(&hash_secret(&code), &origin, name, expires_at)?;
    Ok(PairingGrant {
        code,
        origin,
        expires_at,
    })
}

pub fn issue_local_token(db: &Database, name: &str, days: i64) -> Result<IssuedToken> {
    if !(1..=MAX_CLIENT_DAYS).contains(&days) {
        bail!("token lifetime must be between 1 and {MAX_CLIENT_DAYS} days");
    }
    issue_token(db, name, ClientKind::Local, None, days)
}

pub fn consume_pairing_code(
    db: &Database,
    code: &str,
    request_origin: &str,
) -> Result<IssuedToken, AppError> {
    let origin = normalize_origin(request_origin).map_err(|_| AppError::Forbidden)?;
    let record = db
        .consume_pairing_code(&hash_secret(code), &origin, Utc::now().timestamp())
        .map_err(AppError::internal)?
        .ok_or(AppError::Unauthorized)?;
    debug_assert_eq!(record.origin, origin);
    issue_token(db, &record.name, ClientKind::Browser, Some(&origin), 30)
        .map_err(AppError::internal)
}

pub fn rotate_token(db: &Database, client_id: &str, days: i64) -> Result<IssuedToken> {
    if !(1..=MAX_CLIENT_DAYS).contains(&days) {
        bail!("token lifetime must be between 1 and {MAX_CLIENT_DAYS} days");
    }
    let client = db.get_client(client_id)?.context("client does not exist")?;
    let token = new_token()?;
    let expires_at = Utc::now().timestamp() + days * 86_400;
    db.rotate_client_token(client_id, &hash_secret(&token), expires_at)?;
    Ok(IssuedToken {
        client_id: client.id,
        token,
        expires_at,
    })
}

fn issue_token(
    db: &Database,
    name: &str,
    kind: ClientKind,
    origin: Option<&str>,
    days: i64,
) -> Result<IssuedToken> {
    let token = new_token()?;
    let client_id = Uuid::new_v4().to_string();
    let expires_at = Utc::now().timestamp() + days * 86_400;
    db.insert_client(
        &client_id,
        name,
        kind,
        origin,
        &hash_secret(&token),
        expires_at,
    )?;
    Ok(IssuedToken {
        client_id,
        token,
        expires_at,
    })
}

pub fn authenticate(db: &Database, headers: &HeaderMap) -> Result<AuthenticatedClient, AppError> {
    let value = headers
        .get(http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .filter(|value| value.starts_with(TOKEN_PREFIX))
        .ok_or(AppError::Unauthorized)?;
    let client = db
        .find_client_by_token_hash(&hash_secret(value), Utc::now().timestamp())
        .map_err(AppError::internal)?
        .ok_or(AppError::Unauthorized)?;
    let request_origin = headers
        .get(http::header::ORIGIN)
        .and_then(|value| value.to_str().ok());
    match (&client.kind, &client.origin, request_origin) {
        (ClientKind::Browser, Some(expected), Some(actual))
            if constant_time_eq(expected.as_bytes(), actual.as_bytes()) => {}
        (ClientKind::Browser, _, _) | (ClientKind::Local, _, Some(_)) => {
            return Err(AppError::Forbidden);
        }
        (ClientKind::Local, _, None) => {}
    }
    Ok(client)
}

pub fn hash_secret(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}

fn new_token() -> Result<String> {
    Ok(format!(
        "{TOKEN_PREFIX}{}",
        URL_SAFE_NO_PAD.encode(random_bytes::<32>()?)
    ))
}

fn random_bytes<const N: usize>() -> Result<[u8; N]> {
    let mut bytes = [0_u8; N];
    fill(&mut bytes)
        .map_err(|error| anyhow::anyhow!("secure random generator failed: {error:?}"))?;
    Ok(bytes)
}

fn format_pairing_code(bytes: &[u8; 16]) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut encoded = String::with_capacity(32);
    for byte in bytes {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    format!(
        "PL-{}-{}-{}-{}",
        &encoded[0..8],
        &encoded[8..16],
        &encoded[16..24],
        &encoded[24..32]
    )
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    left.len() == right.len() && bool::from(left.ct_eq(right))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_https_origin() {
        assert_eq!(
            normalize_origin("https://example.com").expect("valid origin"),
            "https://example.com"
        );
        assert_eq!(
            normalize_origin("http://localhost:5173").expect("valid local origin"),
            "http://localhost:5173"
        );
    }

    #[test]
    fn rejects_dangerous_origin_shapes() {
        for origin in [
            "http://example.com",
            "https://example.com/path",
            "https://user@example.com",
            "https://example.com?x=1",
            "file:///tmp/index.html",
        ] {
            assert!(normalize_origin(origin).is_err(), "{origin} should fail");
        }
    }

    #[test]
    fn pairing_codes_have_no_ambiguous_separators() {
        let code = format_pairing_code(&[7; 16]);
        assert_eq!(code, "PL-07070707-07070707-07070707-07070707");
        assert_eq!(code.split('-').count(), 5);
    }

    #[test]
    fn pairing_codes_encode_all_128_bits() {
        let mut bytes = [0; 16];
        let zero_code = format_pairing_code(&bytes);
        bytes[15] = 0xff;
        let last_bit_group_changed = format_pairing_code(&bytes);

        assert_ne!(zero_code, last_bit_group_changed);
        assert!(last_bit_group_changed.ends_with("000000FF"));
    }
}
