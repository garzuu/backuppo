use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use backuppo_core::config::{UpdateChannel, UpdateConfig, UpdateInstallMode};
use base64::Engine;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct UpdateManifest {
    pub sequence: u64,
    pub channel: String,
    pub version: String,
    #[serde(default)]
    pub restic_version: Option<String>,
    #[serde(default)]
    pub min_version: Option<String>,
    pub artifacts: Vec<UpdateArtifact>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct UpdateArtifact {
    pub target: String,
    pub url: String,
    pub sha256: String,
    pub size: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct UpdateState {
    #[serde(default)]
    pub highest_sequence: u64,
    #[serde(default)]
    pub available_version: Option<String>,
    #[serde(default)]
    pub staged_path: Option<PathBuf>,
    #[serde(default)]
    pub staged_sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct UpdateStatus {
    pub current_version: String,
    pub channel: UpdateChannel,
    pub install_mode: UpdateInstallMode,
    pub target: String,
    pub available_version: Option<String>,
    pub staged: bool,
}

pub(crate) struct CheckedUpdate {
    pub manifest: UpdateManifest,
    pub artifact: UpdateArtifact,
}

pub(crate) async fn check(settings: &UpdateConfig) -> Result<Option<CheckedUpdate>> {
    validate_url(&settings.manifest_url)?;
    let client = reqwest::Client::builder()
        .https_only(!is_local_http(&settings.manifest_url))
        .build()?;
    let manifest_bytes = client
        .get(&settings.manifest_url)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    let signature_bytes = client
        .get(format!("{}.sig", settings.manifest_url))
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    let manifest = verify_manifest(
        &manifest_bytes,
        &signature_bytes,
        std::iter::once(settings.public_key.as_str())
            .chain(settings.recovery_public_keys.iter().map(String::as_str)),
    )?;
    validate_manifest(settings, &manifest)?;

    let current = Version::parse(env!("CARGO_PKG_VERSION"))?;
    let offered = Version::parse(&manifest.version)
        .with_context(|| format!("versione manifest non valida: {}", manifest.version))?;
    let mut state = load_state(settings)?;
    if manifest.sequence < state.highest_sequence {
        bail!(
            "manifest obsoleto: sequenza {} inferiore a {}",
            manifest.sequence,
            state.highest_sequence
        );
    }
    state.highest_sequence = state.highest_sequence.max(manifest.sequence);
    if offered <= current {
        state.available_version = None;
        save_state(settings, &state)?;
        return Ok(None);
    }
    let target = platform_target();
    let artifact = manifest
        .artifacts
        .iter()
        .find(|artifact| artifact.target == target)
        .cloned()
        .with_context(|| format!("release {} non disponibile per {target}", manifest.version))?;
    state.available_version = Some(manifest.version.clone());
    save_state(settings, &state)?;
    Ok(Some(CheckedUpdate { manifest, artifact }))
}

pub(crate) async fn download(settings: &UpdateConfig, checked: &CheckedUpdate) -> Result<PathBuf> {
    validate_url(&checked.artifact.url)?;
    let response = reqwest::get(&checked.artifact.url)
        .await?
        .error_for_status()?;
    let bytes = response.bytes().await?;
    if bytes.len() as u64 != checked.artifact.size {
        bail!(
            "dimensione artefatto errata: attesi {}, ricevuti {} byte",
            checked.artifact.size,
            bytes.len()
        );
    }
    let digest = sha256(&bytes);
    if !digest.eq_ignore_ascii_case(&checked.artifact.sha256) {
        bail!("checksum artefatto non valido");
    }
    let directory = PathBuf::from(&settings.download_dir);
    std::fs::create_dir_all(&directory)?;
    let final_path = directory.join(format!(
        "bkpo-{}-{}",
        checked.manifest.version,
        platform_target()
    ));
    let temporary = directory.join(format!(".download-{}", std::process::id()));
    std::fs::write(&temporary, &bytes)?;
    set_executable(&temporary)?;
    std::fs::rename(&temporary, &final_path)?;
    let mut state = load_state(settings)?;
    state.highest_sequence = state.highest_sequence.max(checked.manifest.sequence);
    state.available_version = Some(checked.manifest.version.clone());
    state.staged_path = Some(final_path.clone());
    state.staged_sha256 = Some(digest);
    save_state(settings, &state)?;
    Ok(final_path)
}

pub(crate) fn status(settings: &UpdateConfig) -> Result<UpdateStatus> {
    let state = load_state(settings)?;
    Ok(UpdateStatus {
        current_version: env!("CARGO_PKG_VERSION").into(),
        channel: settings.channel.clone(),
        install_mode: settings.install_mode.clone(),
        target: platform_target(),
        available_version: state.available_version,
        staged: state.staged_path.as_deref().is_some_and(Path::exists),
    })
}

#[cfg(unix)]
pub(crate) fn apply(settings: &UpdateConfig) -> Result<PathBuf> {
    if settings.install_mode != UpdateInstallMode::Standalone {
        bail!("install_mode non e' standalone: usare il package manager o l'orchestratore");
    }
    let state = load_state(settings)?;
    let staged = state
        .staged_path
        .clone()
        .context("nessun aggiornamento scaricato")?;
    let expected = state
        .staged_sha256
        .clone()
        .context("checksum staging mancante")?;
    let bytes = std::fs::read(&staged)?;
    if sha256(&bytes) != expected {
        bail!("il binario in staging non supera la verifica checksum");
    }
    let probe = std::process::Command::new(&staged)
        .arg("--version")
        .output()
        .context("impossibile eseguire il binario in staging")?;
    let advertised = state.available_version.as_deref().unwrap_or_default();
    let version_output = String::from_utf8_lossy(&probe.stdout);
    if !probe.status.success() || !version_output.contains(advertised) {
        bail!("il binario in staging non supera il controllo di avvio/versione");
    }
    let current = std::env::current_exe()?.canonicalize()?;
    let parent = current
        .parent()
        .context("directory del binario non disponibile")?;
    let incoming = parent.join(format!(".bkpo-update-{}", std::process::id()));
    let backup = parent.join(".bkpo-previous");
    std::fs::write(&incoming, bytes)?;
    set_executable(&incoming)?;
    if backup.exists() {
        std::fs::remove_file(&backup)?;
    }
    std::fs::rename(&current, &backup)
        .with_context(|| format!("impossibile salvare '{}'", current.display()))?;
    if let Err(error) = std::fs::rename(&incoming, &current) {
        let _ = std::fs::rename(&backup, &current);
        return Err(error).context("installazione fallita; versione precedente ripristinata");
    }
    let mut next_state = state;
    next_state.available_version = None;
    next_state.staged_path = None;
    next_state.staged_sha256 = None;
    let _ = save_state(settings, &next_state);
    Ok(backup)
}

#[cfg(unix)]
pub(crate) fn rollback(settings: &UpdateConfig) -> Result<PathBuf> {
    if settings.install_mode != UpdateInstallMode::Standalone {
        bail!("install_mode non e' standalone");
    }
    let current = std::env::current_exe()?.canonicalize()?;
    let parent = current
        .parent()
        .context("directory del binario non disponibile")?;
    let backup = parent.join(".bkpo-previous");
    if !backup.exists() {
        bail!("nessuna versione precedente disponibile");
    }
    let failed = parent.join(format!(".bkpo-rollback-{}", std::process::id()));
    std::fs::rename(&current, &failed)?;
    if let Err(error) = std::fs::rename(&backup, &current) {
        let _ = std::fs::rename(&failed, &current);
        return Err(error).context("rollback fallito; versione corrente ripristinata");
    }
    Ok(failed)
}

#[cfg(not(unix))]
pub(crate) fn apply(_settings: &UpdateConfig) -> Result<PathBuf> {
    bail!("l'installazione standalone richiede l'helper di servizio su questa piattaforma")
}

#[cfg(not(unix))]
pub(crate) fn rollback(_settings: &UpdateConfig) -> Result<PathBuf> {
    bail!("il rollback standalone richiede l'helper di servizio su questa piattaforma")
}

fn verify_manifest<'a>(
    bytes: &[u8],
    signature: &[u8],
    public_keys: impl IntoIterator<Item = &'a str>,
) -> Result<UpdateManifest> {
    let decoder = base64::engine::general_purpose::STANDARD;
    let signature_text = std::str::from_utf8(signature)?.trim();
    let signature_bytes = decoder
        .decode(signature_text)
        .context("firma manifest non valida")?;
    let signature = Signature::from_slice(&signature_bytes)?;
    let mut valid = false;
    for public_key in public_keys {
        let Ok(key_bytes) = decoder.decode(public_key.trim()) else {
            continue;
        };
        let Ok(key_array) = <[u8; 32]>::try_from(key_bytes) else {
            continue;
        };
        let Ok(key) = VerifyingKey::from_bytes(&key_array) else {
            continue;
        };
        if key.verify(bytes, &signature).is_ok() {
            valid = true;
            break;
        }
    }
    if !valid {
        bail!("firma del manifest non valida per tutte le chiavi trusted");
    }
    serde_json::from_slice(bytes).context("manifest update non valido")
}

fn validate_manifest(settings: &UpdateConfig, manifest: &UpdateManifest) -> Result<()> {
    let configured = match settings.channel {
        UpdateChannel::Stable => "stable",
        UpdateChannel::Beta => "beta",
        UpdateChannel::Pinned => "stable",
    };
    if manifest.channel != configured {
        bail!(
            "manifest del canale '{}' ricevuto per il canale '{}'",
            manifest.channel,
            configured
        );
    }
    if settings.channel == UpdateChannel::Pinned
        && settings.pinned_version.as_deref() != Some(manifest.version.as_str())
    {
        bail!(
            "la policy richiede la versione {}, il manifest offre {}",
            settings
                .pinned_version
                .as_deref()
                .unwrap_or("non configurata"),
            manifest.version
        );
    }
    if let Some(minimum) = &manifest.min_version {
        let current = Version::parse(env!("CARGO_PKG_VERSION"))?;
        let minimum = Version::parse(minimum)?;
        if current < minimum {
            bail!("aggiornamento diretto non supportato: serve almeno Backuppo {minimum}");
        }
    }
    Ok(())
}

fn platform_target() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

fn state_path(settings: &UpdateConfig) -> PathBuf {
    PathBuf::from(&settings.download_dir).join("state.json")
}

fn load_state(settings: &UpdateConfig) -> Result<UpdateState> {
    let path = state_path(settings);
    if !path.exists() {
        return Ok(UpdateState::default());
    }
    serde_json::from_slice(&std::fs::read(&path)?)
        .with_context(|| format!("stato update '{}' non valido", path.display()))
}

fn save_state(settings: &UpdateConfig, state: &UpdateState) -> Result<()> {
    let path = state_path(settings);
    let parent = path.parent().context("directory update non valida")?;
    std::fs::create_dir_all(parent)?;
    let temporary = parent.join(format!(".state-{}.json", std::process::id()));
    std::fs::write(&temporary, serde_json::to_vec_pretty(state)?)?;
    std::fs::rename(temporary, path)?;
    Ok(())
}

fn sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn validate_url(url: &str) -> Result<()> {
    if url.starts_with("https://") || is_local_http(url) {
        return Ok(());
    }
    bail!("gli aggiornamenti richiedono HTTPS (HTTP ammesso solo per localhost)")
}

fn is_local_http(url: &str) -> bool {
    url.starts_with("http://127.0.0.1:") || url.starts_with("http://localhost:")
}

#[cfg(unix)]
fn set_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = std::fs::metadata(path)?.permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(path, permissions)?;
    Ok(())
}

#[cfg(not(unix))]
fn set_executable(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    #[test]
    fn verifies_exact_manifest_bytes_and_rejects_tampering() {
        let signing = SigningKey::from_bytes(&[7u8; 32]);
        let bytes = br#"{"sequence":1,"channel":"stable","version":"9.0.0","artifacts":[]}"#;
        let signature = signing.sign(bytes);
        let encoder = base64::engine::general_purpose::STANDARD;
        let public = encoder.encode(signing.verifying_key().to_bytes());
        let encoded_signature = encoder.encode(signature.to_bytes());
        assert!(verify_manifest(bytes, encoded_signature.as_bytes(), [&public as &str]).is_ok());
        assert!(verify_manifest(b"{}", encoded_signature.as_bytes(), [&public as &str]).is_err());
    }

    #[test]
    fn only_https_and_local_http_are_allowed() {
        assert!(validate_url("https://updates.example/manifest.json").is_ok());
        assert!(validate_url("http://127.0.0.1:8080/manifest.json").is_ok());
        assert!(validate_url("http://updates.example/manifest.json").is_err());
    }
}
