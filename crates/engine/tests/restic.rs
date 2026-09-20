#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;

use backuppo_core::config::{validate, Config};

#[tokio::test]
async fn restic_engine_backs_up_verifies_and_applies_retention() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let repository = temp.path().join("repository");
    let fake_restic = temp.path().join("restic");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(source.join("data.txt"), "incremental data").unwrap();
    std::fs::write(&fake_restic, FAKE_RESTIC).unwrap();
    let mut permissions = std::fs::metadata(&fake_restic).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&fake_restic, permissions).unwrap();

    let password_env = format!("BACKUPPO_RESTIC_TEST_PASSWORD_{}", std::process::id());
    std::env::set_var(&password_env, "test-password");
    let yaml = format!(
        r#"
destinations:
  snapshots:
    type: restic
    repository: "{}"
    password_env: {}
    binary: "{}"
jobs:
  documents:
    engine: restic
    source: {{ type: folder, path: "{}" }}
    destination: snapshots
    schedule: "0 3 * * *"
    verify_restore: every
    retention: {{ daily: 2 }}
"#,
        repository.display(),
        password_env,
        fake_restic.display(),
        source.display()
    );
    let config = Config::from_yaml(&yaml).unwrap();
    validate(&config).unwrap();

    let artifact = backuppo_engine::run_and_retain("documents", &config)
        .await
        .unwrap();
    assert_eq!(artifact.checksum, "snapshot-test");
    assert!(repository.join("forget-called").exists());

    let event = backuppo_engine::verify_job("documents", &config)
        .await
        .unwrap();
    assert!(event.to_string().contains("snapshot Restic"));
}

const FAKE_RESTIC: &str = r#"#!/bin/sh
set -eu
case "$1" in
  snapshots)
    test -f "$RESTIC_REPOSITORY/snapshot.json"
    cat "$RESTIC_REPOSITORY/snapshot.json"
    ;;
  init)
    mkdir -p "$RESTIC_REPOSITORY"
    ;;
  backup)
    rm -rf "$RESTIC_REPOSITORY/data"
    mkdir -p "$RESTIC_REPOSITORY/data"
    cp -R . "$RESTIC_REPOSITORY/data"
    printf '[{"id":"snapshot-test"}]\n' > "$RESTIC_REPOSITORY/snapshot.json"
    printf '{"message_type":"summary","snapshot_id":"snapshot-test"}\n'
    ;;
  restore)
    shift
    target=''
    while [ "$#" -gt 0 ]; do
      if [ "$1" = '--target' ]; then target="$2"; shift 2; else shift; fi
    done
    mkdir -p "$target"
    cp -R "$RESTIC_REPOSITORY/data/." "$target/"
    ;;
  forget)
    touch "$RESTIC_REPOSITORY/forget-called"
    ;;
  *) exit 2 ;;
esac
"#;
