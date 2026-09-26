use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use ::windows_service::define_windows_service;
use ::windows_service::service::{
    ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus, ServiceType,
};
use ::windows_service::service_control_handler::{self, ServiceControlHandlerResult};
use ::windows_service::service_dispatcher;
use anyhow::{Context, Result};
use tokio::sync::oneshot;

const SERVICE_NAME: &str = "BackuppoHub";
static CONFIG_PATH: OnceLock<PathBuf> = OnceLock::new();

define_windows_service!(ffi_service_main, service_main);

pub(crate) fn run(config_path: PathBuf) -> Result<()> {
    CONFIG_PATH
        .set(config_path)
        .map_err(|_| anyhow::anyhow!("percorso config del servizio già inizializzato"))?;
    service_dispatcher::start(SERVICE_NAME, ffi_service_main)
        .context("impossibile registrare Backuppo Hub presso Service Control Manager")?;
    Ok(())
}

fn service_main(_arguments: Vec<std::ffi::OsString>) {
    if let Err(error) = run_service() {
        eprintln!("servizio Backuppo Hub terminato con errore: {error:#}");
    }
}

fn run_service() -> Result<()> {
    let config_path = CONFIG_PATH
        .get()
        .context("percorso config del servizio non disponibile")?
        .clone();
    let (stop_tx, stop_rx) = oneshot::channel();
    let sender = std::sync::Arc::new(Mutex::new(Some(stop_tx)));
    let handler_sender = std::sync::Arc::clone(&sender);
    let event_handler = move |event| match event {
        ServiceControl::Stop => {
            if let Ok(mut sender) = handler_sender.lock() {
                if let Some(sender) = sender.take() {
                    let _ = sender.send(());
                }
            }
            ServiceControlHandlerResult::NoError
        }
        ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
        _ => ServiceControlHandlerResult::NotImplemented,
    };
    let status_handle = service_control_handler::register(SERVICE_NAME, event_handler)
        .context("registrazione handler del servizio fallita")?;
    status_handle.set_service_status(status(
        ServiceState::Running,
        ServiceControlAccept::STOP,
        ServiceExitCode::Win32(0),
    ))?;

    let runtime = tokio::runtime::Runtime::new().context("creazione runtime fallita")?;
    let result = runtime.block_on(crate::serve_until(&config_path, async move {
        let _ = stop_rx.await;
    }));
    status_handle.set_service_status(status(
        ServiceState::Stopped,
        ServiceControlAccept::empty(),
        if result.is_ok() {
            ServiceExitCode::Win32(0)
        } else {
            ServiceExitCode::ServiceSpecific(1)
        },
    ))?;
    result
}

fn status(
    state: ServiceState,
    controls: ServiceControlAccept,
    exit_code: ServiceExitCode,
) -> ServiceStatus {
    ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: state,
        controls_accepted: controls,
        exit_code,
        checkpoint: 0,
        wait_hint: Duration::default(),
        process_id: None,
    }
}
