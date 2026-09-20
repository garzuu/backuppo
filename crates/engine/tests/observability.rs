use std::collections::HashMap;
use std::fs;

use backuppo_core::config::{
    Compression, Config, DestinationConfig, JobConfig, NotifyConfig, ObservabilityConfig,
    Retention, SourceConfig,
};
use backuppo_engine::history::HistoryStore;
use backuppo_engine::{run_job, verify_job};

fn build_config(root: &std::path::Path) -> Config {
    let source = root.join("source");
    let destination = root.join("destination");
    fs::create_dir_all(&source).unwrap();
    fs::write(source.join("data.txt"), "observable backup").unwrap();

    let mut destinations = HashMap::new();
    destinations.insert(
        "local".to_string(),
        DestinationConfig::Fs {
            root: destination.to_string_lossy().to_string(),
        },
    );
    let mut jobs = HashMap::new();
    jobs.insert(
        "documents".to_string(),
        JobConfig {
            source: SourceConfig::Folder {
                path: source.to_string_lossy().to_string(),
                exclude: Vec::new(),
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
        observability: Some(ObservabilityConfig {
            history_path: root
                .join("state/history.sqlite")
                .to_string_lossy()
                .to_string(),
            status_page: Some(root.join("state/status.html").to_string_lossy().to_string()),
        }),
        reports: Vec::new(),
    }
}

#[tokio::test]
async fn run_and_verify_are_persisted_and_refresh_the_status_page() {
    let temp = tempfile::tempdir().unwrap();
    let config = build_config(temp.path());
    run_job("documents", &config).await.expect("backup");

    let settings = config.observability.as_ref().unwrap();
    let store = HistoryStore::open(&settings.history_path).unwrap();
    let records = store.list(10).unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].kind, "backup");
    assert_eq!(records[0].status, "success");
    assert!(records[0].bytes.is_some());
    assert!(records[0].log.contains("upload confermato"));

    let html = fs::read_to_string(settings.status_page.as_ref().unwrap()).unwrap();
    assert!(html.contains("documents"));
    assert!(html.contains("class=\"status success\">OK"));

    verify_job("documents", &config).await.expect("verify");
    let records = store.list(10).unwrap();
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].kind, "verify");
    assert_eq!(records[0].verification_status, "success");
    assert!(records[0].log.contains("restore verificato"));
}
