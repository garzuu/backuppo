use std::collections::HashMap;
use std::fs;

use backuppo_core::config::{
    Compression, Config, DestinationConfig, JobConfig, NotifyConfig, Retention, SourceConfig,
};
use backuppo_engine::run_and_retain;

const DAY: u64 = 24 * 3600;

fn write_file(path: &std::path::Path, content: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, content).unwrap();
}

fn fake_backup(dst: &std::path::Path, job_name: &str, ts: u64) {
    fs::write(dst.join(format!("{job_name}-{ts}.tar")), b"fake").unwrap();
}

fn backup_count(dst: &std::path::Path) -> usize {
    fs::read_dir(dst).unwrap().count()
}

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
            retention: Retention {
                daily: Some(1),
                weekly: None,
                monthly: None,
            },
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

#[tokio::test]
async fn failed_run_never_triggers_retention() {
    let dst_dir = tempfile::tempdir().unwrap();
    // Backup vecchi preesistenti che una retention "daily: 1" cancellerebbe
    // se venisse invocata.
    let base = 1_700_000_000u64;
    for i in 0..5 {
        fake_backup(dst_dir.path(), "documents", base - i * DAY);
    }
    assert_eq!(backup_count(dst_dir.path()), 5);

    // Sorgente inesistente: il job fallisce prima ancora di caricare nulla.
    let missing_src = std::path::PathBuf::from("/tmp/backuppo-test-does-not-exist-retain");
    let config = build_config(&missing_src, dst_dir.path());

    let err = run_and_retain("documents", &config)
        .await
        .expect_err("il job deve fallire: sorgente inesistente");
    assert!(!err.to_string().is_empty());

    // Retention MAI invocata dopo un fallimento: tutti i vecchi backup
    // restano al loro posto.
    assert_eq!(
        backup_count(dst_dir.path()),
        5,
        "un job fallito non deve mai innescare la retention"
    );
}

#[tokio::test]
async fn successful_run_applies_retention_afterwards() {
    let src_dir = tempfile::tempdir().unwrap();
    let dst_dir = tempfile::tempdir().unwrap();
    write_file(&src_dir.path().join("a.txt"), "contenuto di test");

    // Alcuni backup vecchi preesistenti di giorni diversi, oltre a quello
    // che il job sta per produrre.
    let base = 1_700_000_000u64;
    for i in 1..6 {
        fake_backup(dst_dir.path(), "documents", base - i * DAY);
    }
    assert_eq!(backup_count(dst_dir.path()), 5);

    let config = build_config(src_dir.path(), dst_dir.path());
    run_and_retain("documents", &config)
        .await
        .expect("il job deve avere successo");

    // retention: daily(1) -> deve restare solo il backup appena creato
    // (il più recente, oggi), tutti gli altri giorni vengono ripuliti.
    assert_eq!(
        backup_count(dst_dir.path()),
        1,
        "dopo un successo la retention deve applicarsi e lasciare solo il backup più recente"
    );
}
