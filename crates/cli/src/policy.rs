use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use backuppo_core::config::{Config, DestinationConfig, EngineKind, NotifierConfig};
use backuppo_core::hub_protocol::{PolicyDocument, PolicyEnforcement, SignedPolicy};
use base64::Engine;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Serialize)]
pub(crate) struct PolicyDecision {
    pub sequence: u64,
    pub enforcement: PolicyEnforcement,
    pub expires_at: i64,
    pub violations: Vec<String>,
}

impl PolicyDecision {
    pub(crate) fn blocking_reason(&self) -> Option<String> {
        if self.enforcement == PolicyEnforcement::Block && !self.violations.is_empty() {
            Some(format!(
                "policy #{} non rispettata: {}",
                self.sequence,
                self.violations.join("; ")
            ))
        } else {
            None
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct PolicyState {
    sequence: u64,
    payload_sha256: String,
}

pub(crate) fn verify_and_evaluate(
    signed: &SignedPolicy,
    public_key: &str,
    expected_site_id: i64,
    state_path: &Path,
    config: &Config,
    now: i64,
) -> Result<PolicyDecision> {
    let payload = decode_base64("payload", &signed.payload)?;
    let signature_bytes = decode_base64("signature", &signed.signature)?;
    let public_key_bytes = decode_base64("policy_public_key", public_key)?;
    let key: [u8; 32] = public_key_bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("policy_public_key deve contenere 32 byte"))?;
    let signature_bytes: [u8; 64] = signature_bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("firma policy deve contenere 64 byte"))?;
    VerifyingKey::from_bytes(&key)
        .context("chiave pubblica policy Ed25519 non valida")?
        .verify(&payload, &Signature::from_bytes(&signature_bytes))
        .context("firma policy Ed25519 non valida")?;
    let document: PolicyDocument =
        serde_json::from_slice(&payload).context("payload policy JSON non valido")?;
    if document.site_id != expected_site_id {
        bail!(
            "policy destinata al sito {}, agent configurato per il sito {}",
            document.site_id,
            expected_site_id
        );
    }
    if document.expires_at <= now {
        bail!("policy #{} scaduta", document.sequence);
    }
    let hash = hex(&Sha256::digest(&payload));
    if let Some(previous) = load_state(state_path)? {
        if document.sequence < previous.sequence {
            bail!(
                "rollback policy rifiutato: sequenza {} precedente a {}",
                document.sequence,
                previous.sequence
            );
        }
        if document.sequence == previous.sequence && hash != previous.payload_sha256 {
            bail!(
                "policy #{} riutilizzata con contenuto diverso",
                document.sequence
            );
        }
    }
    let decision = PolicyDecision {
        sequence: document.sequence,
        enforcement: document.enforcement,
        expires_at: document.expires_at,
        violations: evaluate(config, &document),
    };
    save_state(
        state_path,
        &PolicyState {
            sequence: document.sequence,
            payload_sha256: hash,
        },
    )?;
    Ok(decision)
}

fn evaluate(config: &Config, document: &PolicyDocument) -> Vec<String> {
    let mut violations = Vec::new();
    for (job_name, job) in &config.jobs {
        if document.constraints.require_append_only {
            let compliant = job.engine == EngineKind::Restic
                && matches!(
                    config.destinations.get(&job.destination),
                    Some(DestinationConfig::Restic {
                        append_only: true,
                        ..
                    })
                );
            if !compliant {
                violations.push(format!("job '{job_name}' non e' append-only"));
            }
        }
        if document.constraints.require_object_lock {
            let lock = match config.destinations.get(&job.destination) {
                Some(DestinationConfig::Restic { object_lock, .. }) => object_lock.as_ref(),
                _ => None,
            };
            match lock {
                None => violations.push(format!("job '{job_name}' senza Object Lock")),
                Some(lock)
                    if document
                        .constraints
                        .minimum_object_lock_days
                        .is_some_and(|minimum| lock.minimum_retention_days < minimum) =>
                {
                    violations.push(format!(
                        "job '{job_name}' con Object Lock inferiore al minimo"
                    ));
                }
                Some(_) => {}
            }
        }
    }
    if document.constraints.require_signed_updates && config.updates.is_none() {
        violations.push("aggiornamenti firmati non configurati".into());
    }
    if !document.constraints.allow_remote_commands
        && config.notifiers.values().any(|notifier| {
            matches!(
                notifier,
                NotifierConfig::Hub {
                    remote_commands: true,
                    ..
                }
            )
        })
    {
        violations.push("comandi remoti vietati dalla policy".into());
    }
    violations
}

fn decode_base64(field: &str, value: &str) -> Result<Vec<u8>> {
    base64::engine::general_purpose::STANDARD
        .decode(value.trim())
        .with_context(|| format!("{field} non e' base64 valido"))
}

fn load_state(path: &Path) -> Result<Option<PolicyState>> {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .context("stato policy locale non valido"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| format!("lettura '{}'", path.display())),
    }
}

fn save_state(path: &Path, state: &PolicyState) -> Result<()> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)?;
    }
    let temporary = PathBuf::from(format!("{}.tmp", path.display()));
    std::fs::write(&temporary, serde_json::to_vec_pretty(state)?)?;
    std::fs::rename(&temporary, path)?;
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use backuppo_core::hub_protocol::{PolicyConstraints, PolicyEnforcement};
    use ed25519_dalek::{Signer, SigningKey};

    #[test]
    fn verifies_signature_detects_violations_and_rejects_rollback() {
        let temp = tempfile::tempdir().unwrap();
        let state = temp.path().join("policy.json");
        let config = Config::from_yaml(
            r#"
destinations:
  local: { type: fs, root: /tmp/backups }
jobs:
  docs:
    source: { type: folder, path: /tmp/docs }
    destination: local
    schedule: "0 3 * * *"
"#,
        )
        .unwrap();
        let signing = SigningKey::from_bytes(&[7_u8; 32]);
        let policy = |sequence| {
            let payload = serde_json::to_vec(&PolicyDocument {
                sequence,
                site_id: 1,
                issued_at: 100,
                expires_at: 1_000,
                enforcement: PolicyEnforcement::Block,
                constraints: PolicyConstraints {
                    require_append_only: true,
                    ..Default::default()
                },
            })
            .unwrap();
            SignedPolicy {
                signature: base64::engine::general_purpose::STANDARD
                    .encode(signing.sign(&payload).to_bytes()),
                payload: base64::engine::general_purpose::STANDARD.encode(payload),
            }
        };
        let public =
            base64::engine::general_purpose::STANDARD.encode(signing.verifying_key().to_bytes());
        let decision = verify_and_evaluate(&policy(2), &public, 1, &state, &config, 200).unwrap();
        assert!(decision.blocking_reason().unwrap().contains("append-only"));
        assert!(verify_and_evaluate(&policy(1), &public, 1, &state, &config, 200).is_err());
        assert!(verify_and_evaluate(&policy(3), &public, 2, &state, &config, 200).is_err());
    }
}
