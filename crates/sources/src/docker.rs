//! Helper Docker condivisi da `postgres`, `mysql` e `docker_volume`: usano
//! tutti `bollard` per parlare col demone Docker locale.

use std::time::Duration;

use backuppo_core::error::BackupError;
use bollard::exec::{CreateExecOptions, StartExecResults};
use bollard::models::{ContainerCreateBody, HostConfig};
use bollard::query_parameters::{
    CreateContainerOptionsBuilder, CreateImageOptionsBuilder, RemoveContainerOptionsBuilder,
    StartContainerOptions,
};
use bollard::Docker;
use futures_util::StreamExt;
use tokio::io::AsyncWriteExt;
use tracing::info;

/// Connette al demone Docker locale. Pubblica: serve anche a chi deve
/// eseguire comandi in un container gia' in esecuzione (es. il restore di
/// un database), non solo alle sorgenti di questo crate.
pub fn connect() -> Result<Docker, BackupError> {
    Docker::connect_with_local_defaults()
        .map_err(|e| BackupError::Other(format!("impossibile connettersi al demone Docker: {e}")))
}

pub struct ExecResult {
    pub exit_code: i64,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

/// Scarica `image` se non è già presente localmente.
pub(crate) async fn ensure_image(docker: &Docker, image: &str) -> Result<(), BackupError> {
    if docker.inspect_image(image).await.is_ok() {
        return Ok(());
    }

    info!(image, "immagine non trovata localmente, la scarico");
    let options = CreateImageOptionsBuilder::default()
        .from_image(image)
        .build();
    let mut stream = docker.create_image(Some(options), None, None);
    while let Some(item) = stream.next().await {
        item.map_err(|e| BackupError::Other(format!("download immagine '{image}' fallito: {e}")))?;
    }
    Ok(())
}

/// Crea e avvia un container usa-e-getta con `image`, variabili d'ambiente
/// `env` e comando `cmd` (se assente, usa l'entrypoint di default
/// dell'immagine). Ritorna il nome del container creato.
pub(crate) async fn run_throwaway_container(
    docker: &Docker,
    name: &str,
    image: &str,
    env: Vec<String>,
    cmd: Option<Vec<String>>,
    host_config: Option<HostConfig>,
) -> Result<(), BackupError> {
    ensure_image(docker, image).await?;

    let options = CreateContainerOptionsBuilder::default().name(name).build();
    let config = ContainerCreateBody {
        image: Some(image.to_string()),
        env: Some(env),
        cmd,
        host_config,
        ..Default::default()
    };

    docker
        .create_container(Some(options), config)
        .await
        .map_err(|e| BackupError::Other(format!("creazione container '{name}' fallita: {e}")))?;

    docker
        .start_container(name, None::<StartContainerOptions>)
        .await
        .map_err(|e| BackupError::Other(format!("avvio container '{name}' fallito: {e}")))?;

    Ok(())
}

/// Rimuove un container forzatamente (ignora l'errore se già assente).
pub(crate) async fn remove_container(docker: &Docker, name: &str) {
    let options = RemoveContainerOptionsBuilder::default().force(true).build();
    let _ = docker.remove_container(name, Some(options)).await;
}

/// Esegue `cmd` dentro `container`, scrivendo `stdin` (se presente) e
/// attendendone la chiusura, poi raccoglie stdout/stderr ed exit code.
/// Pubblica per lo stesso motivo di [`connect`].
pub async fn exec(
    docker: &Docker,
    container: &str,
    cmd: Vec<String>,
    env: Option<Vec<String>>,
    stdin: Option<&[u8]>,
) -> Result<ExecResult, BackupError> {
    let config = CreateExecOptions {
        cmd: Some(cmd),
        env,
        attach_stdin: Some(stdin.is_some()),
        attach_stdout: Some(true),
        attach_stderr: Some(true),
        ..Default::default()
    };

    let created = docker
        .create_exec(container, config)
        .await
        .map_err(|e| BackupError::Other(format!("creazione exec su '{container}' fallita: {e}")))?;

    let StartExecResults::Attached {
        mut output,
        mut input,
    } = docker
        .start_exec(&created.id, None)
        .await
        .map_err(|e| BackupError::Other(format!("avvio exec su '{container}' fallito: {e}")))?
    else {
        return Err(BackupError::Other(format!(
            "exec su '{container}' avviato in modalità detached inattesa"
        )));
    };

    if let Some(data) = stdin {
        input
            .write_all(data)
            .await
            .map_err(|e| BackupError::Other(format!("scrittura stdin dell'exec fallita: {e}")))?;
    }
    input
        .shutdown()
        .await
        .map_err(|e| BackupError::Other(format!("chiusura stdin dell'exec fallita: {e}")))?;
    drop(input);

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    while let Some(item) = output.next().await {
        match item.map_err(|e| BackupError::Other(format!("lettura output exec fallita: {e}")))? {
            bollard::container::LogOutput::StdOut { message } => stdout.extend_from_slice(&message),
            bollard::container::LogOutput::StdErr { message } => stderr.extend_from_slice(&message),
            _ => {}
        }
    }

    let inspect = docker
        .inspect_exec(&created.id)
        .await
        .map_err(|e| BackupError::Other(format!("inspect exec su '{container}' fallito: {e}")))?;

    Ok(ExecResult {
        exit_code: inspect.exit_code.unwrap_or(-1),
        stdout,
        stderr,
    })
}

/// Riprova `probe` (tipicamente un comando di readiness come `pg_isready`)
/// finché non ha successo o scadono i tentativi.
pub(crate) async fn wait_ready<F, Fut>(
    attempts: u32,
    delay: Duration,
    probe: F,
) -> Result<(), BackupError>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<bool, BackupError>>,
{
    for attempt in 0..attempts {
        if probe().await? {
            return Ok(());
        }
        if attempt + 1 < attempts {
            tokio::time::sleep(delay).await;
        }
    }
    Err(BackupError::Other(
        "il servizio nel container non è diventato pronto entro il timeout".to_string(),
    ))
}

/// Genera un nome di container usa-e-getta univoco per evitare collisioni
/// tra esecuzioni concorrenti.
pub(crate) fn unique_container_name(prefix: &str) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("bkpo-{prefix}-{nanos}")
}
