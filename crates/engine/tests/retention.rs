use std::collections::HashMap;
use std::fs;

use backuppo_core::config::{
    Compression, Config, DestinationConfig, JobConfig, NotifyConfig, Retention, SourceConfig,
};
use backuppo_engine::retention::apply;

const DAY: u64 = 24 * 3600;

fn build_config(dst: &std::path::Path, retention: Retention) -> Config {
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
            source: SourceConfig::Folder {
                path: "/unused".to_string(),
                exclude: vec![],
            },
            destination: "local".to_string(),
            compression: Some(Compression::None),
            encryption: None,
            schedule: "0 3 * * *".to_string(),
            verify_restore: None,
            max_backup_age_hours: None,
            retention,
            notify: NotifyConfig::default(),
        },
    );

    Config {
        destinations,
        notifiers: HashMap::new(),
        jobs,
    }
}

/// Crea un file fittizio col nome che `run_job` avrebbe usato per un backup
/// di `job_name` fatto al timestamp `ts`. Il contenuto non conta: la
/// retention lavora solo sul nome/timestamp.
fn fake_backup(dst: &std::path::Path, job_name: &str, ts: u64) {
    fs::write(dst.join(format!("{job_name}-{ts}.tar")), b"fake").unwrap();
}

fn backup_names(dst: &std::path::Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dst)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    names.sort();
    names
}

#[tokio::test]
async fn does_nothing_without_a_policy() {
    let dst = tempfile::tempdir().unwrap();
    for i in 0..5 {
        fake_backup(dst.path(), "documents", 1_700_000_000 + i * DAY);
    }

    let config = build_config(dst.path(), Retention::default());
    let summary = apply("documents", &config).await.unwrap();

    assert_eq!(summary.deleted, 0);
    assert_eq!(backup_names(dst.path()).len(), 5);
}

#[tokio::test]
async fn never_deletes_the_only_backup() {
    let dst = tempfile::tempdir().unwrap();
    fake_backup(dst.path(), "documents", 1_700_000_000);

    let config = build_config(
        dst.path(),
        Retention {
            daily: Some(1),
            weekly: None,
            monthly: None,
        },
    );
    let summary = apply("documents", &config).await.unwrap();

    assert_eq!(summary.deleted, 0);
    assert_eq!(
        backup_names(dst.path()).len(),
        1,
        "l'unico backup non va mai cancellato"
    );
}

#[tokio::test]
async fn keeps_only_the_configured_number_of_daily_backups() {
    let dst = tempfile::tempdir().unwrap();
    let base = 1_700_000_000u64;
    // Un backup al giorno per 10 giorni.
    for i in 0..10 {
        fake_backup(dst.path(), "documents", base - i * DAY);
    }

    let config = build_config(
        dst.path(),
        Retention {
            daily: Some(3),
            weekly: None,
            monthly: None,
        },
    );
    let summary = apply("documents", &config).await.unwrap();

    assert_eq!(summary.kept, 3);
    assert_eq!(summary.deleted, 7);
    assert_eq!(backup_names(dst.path()).len(), 3);

    // Devono restare i 3 più recenti.
    let remaining = backup_names(dst.path());
    for i in 0..3 {
        let expected = format!("documents-{}.tar", base - i * DAY);
        assert!(
            remaining.contains(&expected),
            "atteso {expected} tra {remaining:?}"
        );
    }
}

#[tokio::test]
async fn daily_and_monthly_policies_combine() {
    let dst = tempfile::tempdir().unwrap();
    let base = 1_700_000_000u64; // fine 2023
                                 // Un backup al giorno per 100 giorni (copre più di 3 mesi).
    for i in 0..100 {
        fake_backup(dst.path(), "documents", base - i * DAY);
    }

    let config = build_config(
        dst.path(),
        Retention {
            daily: Some(2),
            weekly: None,
            monthly: Some(3),
        },
    );
    let summary = apply("documents", &config).await.unwrap();

    // 2 giornalieri + fino a 3 mensili (potrebbero sovrapporsi col più
    // recente): il totale mantenuto deve comunque essere molto minore del
    // totale originale e includere sia i più recenti che backup più vecchi
    // di mesi diversi.
    assert!(summary.kept >= 2, "almeno i bucket giornalieri richiesti");
    assert!(summary.kept <= 5, "non più di daily+monthly bucket");
    assert_eq!(summary.kept + summary.deleted, 100);
}
