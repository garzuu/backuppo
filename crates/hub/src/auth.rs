use anyhow::{Context, Result};
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use rand::rngs::OsRng;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::db::Role;

pub fn hash_password(password: &str) -> Result<String> {
    let salt = SaltString::generate(&mut argon2::password_hash::rand_core::OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|error| anyhow::anyhow!("hashing password fallito: {error}"))
}

pub fn verify_password(password: &str, hash: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(hash) else {
        return false;
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok()
}

/// Genera un token opaco (agent token o refresh token): il valore in
/// chiaro va mostrato una sola volta al chiamante, solo l'hash sha256 va
/// persistito. Nessun modo di recuperare il valore in chiaro dal DB.
pub fn generate_opaque_token() -> (String, String) {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    let plaintext = hex::encode(bytes);
    let hash = hash_opaque_token(&plaintext);
    (plaintext, hash)
}

pub fn hash_opaque_token(plaintext: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(plaintext.as_bytes());
    hex::encode(hasher.finalize())
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub role: String,
    pub exp: i64,
}

pub fn encode_access_token(secret: &str, user_id: i64, role: Role, minutes: i64) -> Result<String> {
    let claims = Claims {
        sub: user_id.to_string(),
        role: role.as_str().to_string(),
        exp: (chrono::Utc::now() + chrono::Duration::minutes(minutes)).timestamp(),
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .context("firma del token JWT fallita")
}

pub fn decode_access_token(secret: &str, token: &str) -> Result<Claims> {
    let data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )
    .context("token JWT non valido o scaduto")?;
    Ok(data.claims)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_and_verifies_a_password() {
        let hash = hash_password("hunter2").unwrap();
        assert!(verify_password("hunter2", &hash));
        assert!(!verify_password("wrong", &hash));
    }

    #[test]
    fn opaque_token_hash_is_reproducible_and_tokens_are_unique() {
        let (plaintext, hash) = generate_opaque_token();
        assert_eq!(hash_opaque_token(&plaintext), hash);

        let (other_plaintext, _) = generate_opaque_token();
        assert_ne!(plaintext, other_plaintext);
    }

    #[test]
    fn access_token_round_trips_and_rejects_wrong_secret() {
        let token = encode_access_token("secret", 42, Role::Admin, 15).unwrap();
        let claims = decode_access_token("secret", &token).unwrap();
        assert_eq!(claims.sub, "42");
        assert_eq!(claims.role, "admin");

        assert!(decode_access_token("altro-secret", &token).is_err());
    }
}
