use std::collections::HashMap;
use std::fs;

use backupper_core::config::{
    Compression, Config, DestinationConfig, EncryptionConfig, JobConfig, NotifyConfig, Retention,
    SourceConfig,
};
use backupper_engine::{archive, run_job};

fn write_file(path: &std::path::Path, content: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, content).unwrap();
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
            source: SourceConfig::Folder {
                path: src.to_string_lossy().to_string(),
                exclude: vec!["*.tmp".to_string()],
            },
            destination: "local".to_string(),
            compression: Some(Compression::Zstd),
            encryption: Some(EncryptionConfig::Age {
                passphrase_env: Some("BACKUPPER_TEST_PASSPHRASE".to_string()),
                key_env: None,
            }),
            schedule: "0 3 * * *".to_string(),
            verify_restore: None,
            max_backup_age_hours: None,
            retention: Retention::default(),
            notify: NotifyConfig::default(),
        },
    );

    Config {
        destinations,
        notifiers: HashMap::new(),
        jobs,
    }
}

/// Confronta ricorsivamente il contenuto di due cartelle (nomi + byte dei file).
fn assert_same_tree(a: &std::path::Path, b: &std::path::Path) {
    let mut entries_a = collect_files(a);
    let mut entries_b = collect_files(b);
    entries_a.sort();
    entries_b.sort();
    assert_eq!(entries_a, entries_b, "l'elenco dei file non corrisponde");

    for rel in entries_a {
        let content_a = fs::read(a.join(&rel)).unwrap();
        let content_b = fs::read(b.join(&rel)).unwrap();
        assert_eq!(content_a, content_b, "contenuto diverso per '{rel}'");
    }
}

fn collect_files(root: &std::path::Path) -> Vec<String> {
    walkdir::WalkDir::new(root)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| {
            e.path()
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/")
        })
        // il manifest è un dettaglio implementativo dell'engine, non fa
        // parte del contenuto originale della sorgente.
        .filter(|rel| rel != backupper_engine::MANIFEST_FILENAME)
        .collect()
}

#[tokio::test]
async fn folder_to_local_round_trip_is_identical() {
    // SAFETY: test eseguito in isolamento, nessuna race su questa env var.
    unsafe {
        std::env::set_var("BACKUPPER_TEST_PASSPHRASE", "correct horse battery staple");
    }

    let src_dir = tempfile::tempdir().unwrap();
    let dst_dir = tempfile::tempdir().unwrap();
    let restore_dir = tempfile::tempdir().unwrap();

    write_file(&src_dir.path().join("a.txt"), "hello world");
    write_file(&src_dir.path().join("sub/b.txt"), "nested file content");
    write_file(&src_dir.path().join("skip.tmp"), "should not be backed up");

    let config = build_config(src_dir.path(), dst_dir.path());

    let artifact = run_job("documents", &config)
        .await
        .expect("il job deve avere successo");

    assert_eq!(artifact.files, 2, "skip.tmp deve essere escluso");
    assert!(artifact.bytes > 0);
    assert_eq!(artifact.checksum.len(), 64, "checksum sha256 in hex");

    // La destination deve contenere esattamente l'archivio caricato.
    let uploaded: Vec<_> = fs::read_dir(dst_dir.path())
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert_eq!(uploaded.len(), 1);
    let uploaded_path = &uploaded[0];
    assert!(uploaded_path.to_string_lossy().ends_with(".tar.zst.age"));

    let uploaded_size = fs::metadata(uploaded_path).unwrap().len();
    assert_eq!(uploaded_size, artifact.bytes);

    // Il checksum riportato deve corrispondere al file effettivamente caricato.
    let checksum = archive::sha256_file(uploaded_path).unwrap();
    assert_eq!(checksum, artifact.checksum);

    // Round-trip: decifra + decomprime e confronta col sorgente originale
    // (escludendo skip.tmp, che non doveva finire nel backup).
    archive::extract_archive(
        uploaded_path,
        restore_dir.path(),
        true,
        Some("correct horse battery staple"),
    )
    .unwrap();

    fs::remove_file(src_dir.path().join("skip.tmp")).unwrap();
    assert_same_tree(src_dir.path(), restore_dir.path());
}

#[tokio::test]
async fn wrong_passphrase_fails_to_restore() {
    unsafe {
        std::env::set_var("BACKUPPER_TEST_PASSPHRASE_2", "the-real-passphrase");
    }

    let src_dir = tempfile::tempdir().unwrap();
    let dst_dir = tempfile::tempdir().unwrap();
    write_file(&src_dir.path().join("a.txt"), "secret content");

    let mut config = build_config(src_dir.path(), dst_dir.path());
    if let Some(job) = config.jobs.get_mut("documents") {
        job.encryption = Some(EncryptionConfig::Age {
            passphrase_env: Some("BACKUPPER_TEST_PASSPHRASE_2".to_string()),
            key_env: None,
        });
    }

    let artifact = run_job("documents", &config).await.unwrap();
    let uploaded = dst_dir.path().join(artifact.path.file_name().unwrap());

    let restore_dir = tempfile::tempdir().unwrap();
    let result = archive::extract_archive(
        &uploaded,
        restore_dir.path(),
        true,
        Some("wrong-passphrase"),
    );
    assert!(result.is_err(), "una passphrase sbagliata deve fallire");
}
