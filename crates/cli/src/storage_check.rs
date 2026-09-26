use anyhow::{bail, Context, Result};
use backuppo_core::config::{ObjectLockMode, S3ObjectLockConfig};
use backuppo_core::secrets::resolve_env;
use chrono::Utc;
use reqwest::{Client, Url};
use serde::Serialize;
use sha2::{Digest, Sha256};

#[derive(Debug, Serialize)]
pub(crate) struct ObjectLockReport {
    pub bucket: String,
    pub enabled: bool,
    pub mode: String,
    pub retention_days: u32,
}

pub(crate) async fn verify_object_lock(settings: &S3ObjectLockConfig) -> Result<ObjectLockReport> {
    let access_key = resolve_env("object_lock.access_key_id_env", &settings.access_key_id_env)?;
    let secret_key = resolve_env(
        "object_lock.secret_access_key_env",
        &settings.secret_access_key_env,
    )?;
    let session_token = settings
        .session_token_env
        .as_deref()
        .map(|name| resolve_env("object_lock.session_token_env", name))
        .transpose()?;
    let endpoint = settings
        .endpoint
        .clone()
        .unwrap_or_else(|| format!("https://s3.{}.amazonaws.com", settings.region));
    let mut url = Url::parse(&endpoint).context("endpoint S3 Object Lock non valido")?;
    if settings.virtual_host_style {
        let host = url.host_str().context("endpoint S3 senza host")?;
        url.set_host(Some(&format!("{}.{}", settings.bucket, host)))
            .context("bucket non valido per virtual-host style")?;
    } else {
        let path = format!("{}/{}", url.path().trim_end_matches('/'), settings.bucket);
        url.set_path(&path);
    }
    url.set_query(Some("object-lock="));

    let now = Utc::now();
    let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
    let date = now.format("%Y%m%d").to_string();
    let payload_hash = hex(&Sha256::digest([]));
    let host = match url.port() {
        Some(port) => format!("{}:{port}", url.host_str().unwrap_or_default()),
        None => url.host_str().unwrap_or_default().to_string(),
    };

    let mut canonical_headers =
        format!("host:{host}\nx-amz-content-sha256:{payload_hash}\nx-amz-date:{amz_date}\n");
    let mut signed_headers = "host;x-amz-content-sha256;x-amz-date".to_string();
    if let Some(token) = &session_token {
        canonical_headers.push_str(&format!("x-amz-security-token:{}\n", token.trim()));
        signed_headers.push_str(";x-amz-security-token");
    }
    let canonical_request = format!(
        "GET\n{}\nobject-lock=\n{canonical_headers}\n{signed_headers}\n{payload_hash}",
        url.path()
    );
    let scope = format!("{date}/{}/s3/aws4_request", settings.region);
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{amz_date}\n{scope}\n{}",
        hex(&Sha256::digest(canonical_request.as_bytes()))
    );
    let date_key = hmac_sha256(format!("AWS4{secret_key}").as_bytes(), date.as_bytes());
    let region_key = hmac_sha256(&date_key, settings.region.as_bytes());
    let service_key = hmac_sha256(&region_key, b"s3");
    let signing_key = hmac_sha256(&service_key, b"aws4_request");
    let signature = hex(&hmac_sha256(&signing_key, string_to_sign.as_bytes()));
    let authorization = format!(
        "AWS4-HMAC-SHA256 Credential={access_key}/{scope}, SignedHeaders={signed_headers}, Signature={signature}"
    );

    let mut request = Client::new()
        .get(url)
        .header("x-amz-content-sha256", payload_hash)
        .header("x-amz-date", amz_date)
        .header("authorization", authorization);
    if let Some(token) = session_token {
        request = request.header("x-amz-security-token", token);
    }
    let response = request
        .send()
        .await
        .context("verifica Object Lock fallita")?;
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if !status.is_success() {
        bail!(
            "S3 ha rifiutato GetObjectLockConfiguration ({status}): {}",
            body.trim()
        );
    }

    let enabled = xml_value(&body, "ObjectLockEnabled").as_deref() == Some("Enabled");
    let mode = xml_value(&body, "Mode").context("policy Object Lock senza Mode")?;
    let retention_days = if let Some(days) = xml_value(&body, "Days") {
        days.parse::<u32>().context("Days Object Lock non valido")?
    } else if let Some(years) = xml_value(&body, "Years") {
        years
            .parse::<u32>()
            .context("Years Object Lock non valido")?
            .saturating_mul(365)
    } else {
        bail!("policy Object Lock senza retention predefinita Days/Years");
    };
    let expected = match settings.expected_mode {
        ObjectLockMode::Governance => "GOVERNANCE",
        ObjectLockMode::Compliance => "COMPLIANCE",
    };
    if !enabled {
        bail!(
            "Object Lock non e' abilitato sul bucket '{}';",
            settings.bucket
        );
    }
    if mode != expected {
        bail!("modalita Object Lock {mode}, attesa {expected}");
    }
    if retention_days < settings.minimum_retention_days {
        bail!(
            "retention Object Lock di {retention_days} giorni, minimo richiesto {}",
            settings.minimum_retention_days
        );
    }
    Ok(ObjectLockReport {
        bucket: settings.bucket.clone(),
        enabled,
        mode,
        retention_days,
    })
}

fn xml_value(xml: &str, tag: &str) -> Option<String> {
    let start = format!("<{tag}>");
    let end = format!("</{tag}>");
    let value_start = xml.find(&start)? + start.len();
    let value_end = xml[value_start..].find(&end)? + value_start;
    Some(xml[value_start..value_end].trim().to_string())
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
    const BLOCK: usize = 64;
    let mut normalized = [0_u8; BLOCK];
    if key.len() > BLOCK {
        normalized[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        normalized[..key.len()].copy_from_slice(key);
    }
    let mut inner_key = [0x36_u8; BLOCK];
    let mut outer_key = [0x5c_u8; BLOCK];
    for index in 0..BLOCK {
        inner_key[index] ^= normalized[index];
        outer_key[index] ^= normalized[index];
    }
    let inner = Sha256::new()
        .chain_update(inner_key)
        .chain_update(data)
        .finalize();
    Sha256::new()
        .chain_update(outer_key)
        .chain_update(inner)
        .finalize()
        .into()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_object_lock_response() {
        let xml = "<ObjectLockConfiguration><ObjectLockEnabled>Enabled</ObjectLockEnabled><Rule><DefaultRetention><Mode>COMPLIANCE</Mode><Days>30</Days></DefaultRetention></Rule></ObjectLockConfiguration>";
        assert_eq!(xml_value(xml, "Mode").as_deref(), Some("COMPLIANCE"));
        assert_eq!(xml_value(xml, "Days").as_deref(), Some("30"));
    }

    #[test]
    fn hmac_matches_known_vector() {
        assert_eq!(
            hex(&hmac_sha256(
                b"key",
                b"The quick brown fox jumps over the lazy dog"
            )),
            "f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8"
        );
    }

    #[tokio::test]
    async fn verifies_policy_through_a_signed_s3_request() {
        use std::io::{Read, Write};
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut bytes = [0_u8; 8192];
            let read = stream.read(&mut bytes).unwrap();
            let request = String::from_utf8_lossy(&bytes[..read]);
            assert!(request.starts_with("GET /locked?object-lock="));
            assert!(request
                .to_ascii_lowercase()
                .contains("authorization: aws4-hmac-sha256"));
            let body = "<ObjectLockConfiguration><ObjectLockEnabled>Enabled</ObjectLockEnabled><Rule><DefaultRetention><Mode>COMPLIANCE</Mode><Days>30</Days></DefaultRetention></Rule></ObjectLockConfiguration>";
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
        });
        let suffix = std::process::id();
        let access_env = format!("BACKUPPO_LOCK_ACCESS_{suffix}");
        let secret_env = format!("BACKUPPO_LOCK_SECRET_{suffix}");
        std::env::set_var(&access_env, "access");
        std::env::set_var(&secret_env, "secret");
        let report = verify_object_lock(&S3ObjectLockConfig {
            bucket: "locked".into(),
            region: "eu-central-1".into(),
            endpoint: Some(format!("http://{address}")),
            access_key_id_env: access_env,
            secret_access_key_env: secret_env,
            session_token_env: None,
            virtual_host_style: false,
            expected_mode: ObjectLockMode::Compliance,
            minimum_retention_days: 30,
        })
        .await
        .unwrap();
        server.join().unwrap();
        assert!(report.enabled);
        assert_eq!(report.retention_days, 30);
    }
}
