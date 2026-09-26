use std::collections::HashMap;

use backuppo_core::config::{
    Compression, Config, DestinationConfig, JobConfig, NotifyConfig, Retention, SourceConfig,
};
use backuppo_core::model::{OverwritePolicy, RestoreRequest};

fn config(source: &std::path::Path, destination: &std::path::Path) -> Config {
    let mut destinations = HashMap::new();
    destinations.insert(
        "local".into(),
        DestinationConfig::Fs {
            root: destination.to_string_lossy().to_string(),
        },
    );
    let mut jobs = HashMap::new();
    jobs.insert(
        "documents".into(),
        JobConfig {
            source: SourceConfig::Folder {
                path: source.to_string_lossy().to_string(),
                exclude: Vec::new(),
            },
            destination: "local".into(),
            engine: Default::default(),
            compression: Some(Compression::Zstd),
            encryption: None,
            schedule: "0 3 * * *".into(),
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

#[tokio::test]
async fn lists_browses_and_selectively_restores_an_archive() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let destination = root.path().join("backups");
    std::fs::create_dir_all(source.join("docs")).unwrap();
    std::fs::write(source.join("docs/a.txt"), b"alpha").unwrap();
    std::fs::write(source.join("other.txt"), b"other").unwrap();
    let config = config(&source, &destination);
    backuppo_engine::run_job("documents", &config)
        .await
        .expect("backup");

    let snapshots = backuppo_engine::snapshots("documents", &config)
        .await
        .expect("snapshots");
    assert_eq!(snapshots.len(), 1);
    let entries = backuppo_engine::browse("documents", &config, "latest")
        .await
        .expect("browse");
    assert!(entries.iter().any(|entry| entry.path == "docs/a.txt"));

    let target = root.path().join("restore");
    let request = RestoreRequest {
        snapshot: "latest".into(),
        target: target.clone(),
        include: vec!["docs".into()],
        dry_run: false,
        overwrite: OverwritePolicy::Never,
    };
    let result = backuppo_engine::restore("documents", &config, &request)
        .await
        .expect("restore");
    assert_eq!(result.files, 1);
    assert_eq!(std::fs::read(target.join("docs/a.txt")).unwrap(), b"alpha");
    assert!(!target.join("other.txt").exists());
}

#[tokio::test]
async fn refuses_a_non_empty_target_without_overwrite_permission() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let destination = root.path().join("backups");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(source.join("data.txt"), b"data").unwrap();
    let config = config(&source, &destination);
    backuppo_engine::run_job("documents", &config)
        .await
        .unwrap();
    let target = root.path().join("restore");
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(target.join("keep.txt"), b"keep").unwrap();
    let result = backuppo_engine::restore(
        "documents",
        &config,
        &RestoreRequest {
            snapshot: "latest".into(),
            target,
            include: Vec::new(),
            dry_run: false,
            overwrite: OverwritePolicy::Never,
        },
    )
    .await;
    assert!(result.is_err());
}
