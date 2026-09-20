use std::collections::HashMap;
use std::fs;

use backuppo_core::config::{
    Compression, Config, DestinationConfig, JobConfig, NotifierConfig, NotifyConfig, Retention,
    SourceConfig,
};
use backuppo_engine::run_job;
use tokio::net::TcpListener;
use tokio::sync::oneshot;

// Il notifier SMTP reale negozia sempre STARTTLS (`lettre::relay`), quindi
// non possiamo far finta di essere un server SMTP completo senza
// implementare anche TLS. Qui ci basta però osservare che una connessione
// TCP è stata *tentata* verso il canale giusto: la correttezza del
// protocollo SMTP/Telegram è già testata con mock server in
// crates/notifiers. Questo test verifica solo il fan-out per esito del job,
// cioè il requisito della Fase 4: un job fallito notifica più canali, uno
// riuscito solo quelli di `on_success`.
async fn spawn_connection_probe() -> (u16, oneshot::Receiver<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let (tx, rx) = oneshot::channel();

    tokio::spawn(async move {
        let _ = listener.accept().await;
        let _ = tx.send(());
    });

    (port, rx)
}

fn write_file(path: &std::path::Path, content: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, content).unwrap();
}

fn smtp_notifier(port: u16) -> NotifierConfig {
    NotifierConfig::Smtp {
        host: "127.0.0.1".to_string(),
        port,
        user: "backups".to_string(),
        password_env: "BACKUPPER_TEST_SMTP_PASSWORD".to_string(),
        from: "backups@example.com".to_string(),
        to: vec!["ops@example.com".to_string()],
    }
}

/// Config con un job "documents": `on_success: [primary]`,
/// `on_failure: [primary, escalation]`.
fn build_config(
    src: &std::path::Path,
    dst: &std::path::Path,
    primary_port: u16,
    escalation_port: u16,
) -> Config {
    let mut destinations = HashMap::new();
    destinations.insert(
        "local".to_string(),
        DestinationConfig::Fs {
            root: dst.to_string_lossy().to_string(),
        },
    );

    let mut notifiers = HashMap::new();
    notifiers.insert("primary".to_string(), smtp_notifier(primary_port));
    notifiers.insert("escalation".to_string(), smtp_notifier(escalation_port));

    let mut jobs = HashMap::new();
    jobs.insert(
        "documents".to_string(),
        JobConfig {
            engine: Default::default(),
            source: SourceConfig::Folder {
                path: src.to_string_lossy().to_string(),
                exclude: vec![],
            },
            destination: "local".to_string(),
            compression: Some(Compression::None),
            encryption: None,
            schedule: "0 3 * * *".to_string(),
            verify_restore: None,
            max_backup_age_hours: None,
            retention: Retention::default(),
            notify: NotifyConfig {
                on_success: vec!["primary".to_string()],
                on_failure: vec!["primary".to_string(), "escalation".to_string()],
                on_verify: vec![],
            },
            pre: Vec::new(),
            post: Vec::new(),
        },
    );

    Config {
        destinations,
        notifiers,
        jobs,
        observability: None,
        reports: Vec::new(),
        api: None,
    }
}

#[tokio::test]
async fn successful_job_notifies_only_on_success_channels() {
    unsafe {
        std::env::set_var("BACKUPPER_TEST_SMTP_PASSWORD", "unused");
    }

    let (primary_port, primary_probe) = spawn_connection_probe().await;
    let (escalation_port, escalation_probe) = spawn_connection_probe().await;

    let src_dir = tempfile::tempdir().unwrap();
    let dst_dir = tempfile::tempdir().unwrap();
    write_file(&src_dir.path().join("a.txt"), "contenuto di test");

    let config = build_config(
        src_dir.path(),
        dst_dir.path(),
        primary_port,
        escalation_port,
    );

    run_job("documents", &config)
        .await
        .expect("il job deve avere successo");

    tokio::time::timeout(std::time::Duration::from_secs(2), primary_probe)
        .await
        .expect("il canale on_success deve ricevere un tentativo di notifica")
        .unwrap();

    let escalation_result =
        tokio::time::timeout(std::time::Duration::from_millis(300), escalation_probe).await;
    assert!(
        escalation_result.is_err(),
        "un job riuscito non deve notificare i canali fuori da on_success"
    );
}

#[tokio::test]
async fn failed_job_notifies_all_failure_channels() {
    unsafe {
        std::env::set_var("BACKUPPER_TEST_SMTP_PASSWORD", "unused");
    }

    let (primary_port, primary_probe) = spawn_connection_probe().await;
    let (escalation_port, escalation_probe) = spawn_connection_probe().await;

    // Sorgente inesistente: il job fallisce già in fase di preparazione.
    let missing_src = std::path::PathBuf::from("/tmp/backuppo-test-does-not-exist-xyz");
    let dst_dir = tempfile::tempdir().unwrap();
    let config = build_config(&missing_src, dst_dir.path(), primary_port, escalation_port);

    let err = run_job("documents", &config)
        .await
        .expect_err("il job deve fallire: sorgente inesistente");
    assert!(!err.to_string().is_empty());

    for (name, probe) in [("primary", primary_probe), ("escalation", escalation_probe)] {
        tokio::time::timeout(std::time::Duration::from_secs(2), probe)
            .await
            .unwrap_or_else(|_| {
                panic!("il canale '{name}' doveva ricevere un tentativo di notifica")
            })
            .unwrap();
    }
}
