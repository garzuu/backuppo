use anyhow::{Context, Result};
use base64::Engine;
use ed25519_dalek::SigningKey;

pub fn load_signing_key(environment: &str) -> Result<SigningKey> {
    let encoded = std::env::var(environment)
        .with_context(|| format!("variabile policy signing key '{environment}' non impostata"))?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded.trim())
        .context("policy signing key non e' base64 valida")?;
    let key: [u8; 32] = bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("policy signing key deve contenere 32 byte"))?;
    Ok(SigningKey::from_bytes(&key))
}
