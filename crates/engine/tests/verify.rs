use std::collections::HashMap;
use std::fs;

use backuppo_core::config::{
    Compression, Config, DestinationConfig, JobConfig, NotifyConfig, Retention, SourceConfig,
    VerifyRestore,
};
use backuppo_core::model::JobEvent;
use backuppo_engine::{run_job, verify_job};

fn write_file(path: &std::path::Path, content: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, content).unwrap();
}

/// Job senza compressione né cifratura: l'archivio è un tar semplice, così
/// alterare un byte nel mezzo modifica silenziosamente il contenuto di un
/// file senza rompere il formato tar né essere già intercettato da zstd/age.
fn build_config(src: &std::path::Path, dst: &std::path::Path) -> Config {
    let mut destinations = HashMap::new();
    destinations.insert(
        "local".to_string(),
        DestinationConfig::Fs {
            root: dst.to_string_lossy().to_string(),
        },
    );

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
            notify: NotifyConfig::default(),
            pre: Vec::new(),
            post: Vec::new(),
        },
    );

    Config {
        destinations,
        notifiers: HashMap::new(),
        jobs,
        observability: None,
        reports: Vec::new(),
        api: None,
        updates: None,
    }
}

fn uploaded_archive_path(dst: &std::path::Path) -> std::path::PathBuf {
    fs::read_dir(dst)
        .unwrap()
        .map(|e| e.unwrap().path())
        .next()
        .expect("un archivio deve essere stato caricato")
}

#[tokio::test]
async fn verify_succeeds_on_untouched_backup() {
    let src_dir = tempfile::tempdir().unwrap();
    let dst_dir = tempfile::tempdir().unwrap();

    write_file(
        &src_dir.path().join("a.txt"),
        "contenuto originale del file a",
    );
    write_file(
        &src_dir.path().join("sub/b.txt"),
        "contenuto originale del file b, un po' più lungo",
    );

    let config = build_config(src_dir.path(), dst_dir.path());
    run_job("documents", &config)
        .await
        .expect("il backup deve avere successo");

    let event = verify_job("documents", &config)
        .await
        .expect("la verifica di un backup integro deve avere successo");

    match event {
        JobEvent::RestoreVerified { job, detail } => {
            assert_eq!(job, "documents");
            assert!(detail.contains("2 file"), "detail: {detail}");
        }
        other => panic!("evento inatteso: {other:?}"),
    }
}

#[tokio::test]
async fn verify_fails_clearly_on_corrupted_backup() {
    let src_dir = tempfile::tempdir().unwrap();
    let dst_dir = tempfile::tempdir().unwrap();

    write_file(
        &src_dir.path().join("a.txt"),
        "contenuto sufficientemente lungo da sopravvivere a un flip di un byte senza sparire",
    );

    let config = build_config(src_dir.path(), dst_dir.path());
    run_job("documents", &config)
        .await
        .expect("il backup deve avere successo");

    // Corrompe di proposito un byte in mezzo all'archivio caricato.
    let archive_path = uploaded_archive_path(dst_dir.path());
    let mut bytes = fs::read(&archive_path).unwrap();
    let mid = bytes.len() / 2;
    bytes[mid] ^= 0xFF;
    fs::write(&archive_path, bytes).unwrap();

    let err = verify_job("documents", &config)
        .await
        .expect_err("la verifica di un backup corrotto deve fallire");

    let message = err.to_string();
    assert!(
        message.contains("documents"),
        "il messaggio deve nominare il job: {message}"
    );
    assert!(
        message.contains("corrotto")
            || message.contains("dimensione diversa")
            || message.contains("verifica del restore fallita")
            || message.contains("impossibile ripristinare"),
        "il messaggio deve spiegare che il backup è corrotto o non ripristinabile: {message}"
    );
}

#[tokio::test]
async fn run_with_verify_restore_every_succeeds_and_verifies() {
    let src_dir = tempfile::tempdir().unwrap();
    let dst_dir = tempfile::tempdir().unwrap();
    write_file(
        &src_dir.path().join("a.txt"),
        "contenuto del job con verify_restore=every",
    );

    let mut config = build_config(src_dir.path(), dst_dir.path());
    config.jobs.get_mut("documents").unwrap().verify_restore = Some(VerifyRestore::Every);

    // "Un backup si considera riuscito solo dopo che l'upload è confermato
    // e (se attivo) il restore è verificato": con verify_restore=every la
    // verifica avviene dentro run_job stesso.
    run_job("documents", &config)
        .await
        .expect("un backup integro con verify_restore=every deve avere successo");
}

#[tokio::test]
async fn verify_fails_when_no_backup_exists() {
    let src_dir = tempfile::tempdir().unwrap();
    let dst_dir = tempfile::tempdir().unwrap();
    let config = build_config(src_dir.path(), dst_dir.path());

    let err = verify_job("documents", &config)
        .await
        .expect_err("senza backup la verifica deve fallire");
    assert!(err.to_string().contains("nessun backup trovato"));
}
