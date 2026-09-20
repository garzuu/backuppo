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

const SERVICE_NAME: &str = "Backuppo";
static CONFIG_PATH: OnceLock<PathBuf> = OnceLock::new();

define_windows_service!(ffi_service_main, service_main);

pub(crate) fn run(config_path: PathBuf) -> Result<()> {
    CONFIG_PATH
        .set(config_path)
        .map_err(|_| anyhow::anyhow!("percorso config del servizio già inizializzato"))?;
    service_dispatcher::start(SERVICE_NAME, ffi_service_main)
        .context("impossibile registrare Backuppo presso Service Control Manager")?;
    Ok(())
}

fn service_main(_arguments: Vec<std::ffi::OsString>) {
    if let Err(error) = run_service() {
        eprintln!("servizio Backuppo terminato con errore: {error:#}");
    }
}

fn run_service() -> Result<()> {
    let config_path = CONFIG_PATH
        .get()
        .context("percorso config del servizio non disponibile")?
        .clone();
    let config = crate::load_config(&config_path)?;
    let (stop_tx, stop_rx) = oneshot::channel();
    let stop_tx = std::sync::Arc::new(Mutex::new(Some(stop_tx)));
    let handler_sender = std::sync::Arc::clone(&stop_tx);

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

    status_handle
        .set_service_status(service_status(
            ServiceState::Running,
            ServiceControlAccept::STOP,
            ServiceExitCode::Win32(0),
        ))
        .context("impossibile impostare lo stato Running")?;

    let runtime =
        tokio::runtime::Runtime::new().context("creazione runtime del servizio fallita")?;
    let result = runtime.block_on(crate::daemon::run_until(config, async move {
        let _ = stop_rx.await;
    }));

    let exit_code = if result.is_ok() {
        ServiceExitCode::Win32(0)
    } else {
        ServiceExitCode::ServiceSpecific(1)
    };
    status_handle
        .set_service_status(service_status(
            ServiceState::Stopped,
            ServiceControlAccept::empty(),
            exit_code,
        ))
        .context("impossibile impostare lo stato Stopped")?;
    result
}

fn service_status(
    current_state: ServiceState,
    controls_accepted: ServiceControlAccept,
    exit_code: ServiceExitCode,
) -> ServiceStatus {
    ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state,
        controls_accepted,
        exit_code,
        checkpoint: 0,
        wait_hint: Duration::default(),
        process_id: None,
    }
}
