//! Test di integrazione "live" contro servizi reali (via Docker) per la
//! Fase 6: prova che lo stesso identico job funzioni verso locale, SFTP e S3
//! cambiando solo la `destination`. Richiede:
//!   docker run -d --name backuppo-sftp-test -p 2222:22 \
//!     -e SFTP_USERS="testuser:testpass:::upload" atmoz/sftp
//!   docker run -d --name backuppo-minio-test -p 9000:9000 \
//!     -e MINIO_ROOT_USER=minioadmin -e MINIO_ROOT_PASSWORD=minioadmin \
//!     quay.io/minio/minio server /data
//!   (poi creare il bucket "backuppo-test" col client `mc`)
//!   docker run -d --name backuppo-webdav-test -p 8080:80 \
//!     -e USERNAME=testuser -e PASSWORD=testpass bytemark/webdav
//!
//! Se i servizi non sono raggiungibili (es. in CI senza Docker), i test
//! stampano un avviso e passano senza verificare nulla: sono pensati per
//! essere eseguiti volontariamente in locale con l'infrastruttura sopra.

use std::collections::HashMap;
use std::fs;
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

use backuppo_core::config::{
    Compression, Config, DestinationConfig, EncryptionConfig, JobConfig, NotifyConfig, Retention,
    RetryConfig, SourceConfig, VerifyRestore,
};
use backuppo_core::model::Artifact;
use backuppo_core::traits::Destination;
use backuppo_engine::{run_job, verify_job};

fn is_reachable(addr: &str) -> bool {
    match addr.parse::<SocketAddr>() {
        Ok(socket_addr) => {
            TcpStream::connect_timeout(&socket_addr, Duration::from_millis(500)).is_ok()
        }
        Err(_) => false,
    }
}

fn write_file(path: &std::path::Path, content: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, content).unwrap();
}

/// Stessa identica config di job per tutti i backend: cambia solo la entry
/// in `destinations` referenziata da `job.destination`.
fn build_config(
    src: &std::path::Path,
    destination_name: &str,
    destination: DestinationConfig,
) -> Config {
    let mut destinations = HashMap::new();
    destinations.insert(destination_name.to_string(), destination);

    let mut jobs = HashMap::new();
    jobs.insert(
        "documents".to_string(),
        JobConfig {
            source: SourceConfig::Folder {
                path: src.to_string_lossy().to_string(),
                exclude: vec![],
            },
            destination: destination_name.to_string(),
            compression: Some(Compression::Zstd),
            encryption: Some(EncryptionConfig::Age {
                passphrase_env: Some("BACKUPPER_REMOTE_TEST_PASSPHRASE".to_string()),
                key_env: None,
            }),
            schedule: "0 3 * * *".to_string(),
            verify_restore: Some(VerifyRestore::Never),
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
    }
}

async fn assert_job_round_trips(config: &Config) {
    unsafe {
        std::env::set_var(
            "BACKUPPER_REMOTE_TEST_PASSPHRASE",
            "correct horse battery staple",
        );
    }

    let artifact = run_job("documents", config)
        .await
        .expect("il job deve avere successo verso questa destination");
    assert!(artifact.bytes > 0);

    let event = verify_job("documents", config)
        .await
        .expect("la verifica del restore deve avere successo");
    let detail = event.to_string();
    assert!(detail.contains("documents"), "detail: {detail}");
}

async fn seed_backup(dest: &dyn Destination, dir: &std::path::Path, job: &str, ts: u64) {
    let path = dir.join(format!("{job}-{ts}.tar"));
    fs::write(&path, b"contenuto fittizio di backup").unwrap();
    let artifact = Artifact {
        path,
        bytes: 27,
        files: 1,
        checksum: String::new(),
    };
    dest.upload(&artifact).await.unwrap();
}

/// Verifica che `retention::apply` sappia elencare e cancellare backup su
/// una destination remota reale (non solo su `fs`, già coperto altrove).
async fn assert_retention_prunes_old_backups(
    job_name: &str,
    make_destination: impl Fn() -> DestinationConfig,
) {
    let dest = backuppo_destinations::build(&make_destination()).expect("destination costruibile");
    let seed_dir = tempfile::tempdir().unwrap();
    let base = 1_700_000_000u64;
    const DAY: u64 = 24 * 3600;
    for i in 0..5u64 {
        seed_backup(dest.as_ref(), seed_dir.path(), job_name, base - i * DAY).await;
    }

    let mut destinations = HashMap::new();
    destinations.insert("dest".to_string(), make_destination());
    let mut jobs = HashMap::new();
    jobs.insert(
        job_name.to_string(),
        JobConfig {
            source: SourceConfig::Folder {
                path: "/unused".to_string(),
                exclude: vec![],
            },
            destination: "dest".to_string(),
            compression: None,
            encryption: None,
            schedule: "0 3 * * *".to_string(),
            verify_restore: None,
            max_backup_age_hours: None,
            retention: Retention {
                daily: Some(2),
                weekly: None,
                monthly: None,
            },
            notify: NotifyConfig::default(),
            pre: Vec::new(),
            post: Vec::new(),
        },
    );
    let config = Config {
        destinations,
        notifiers: HashMap::new(),
        jobs,
    };

    let summary = backuppo_engine::retention::apply(job_name, &config)
        .await
        .expect("la retention deve applicarsi senza errori");
    assert_eq!(summary.kept, 2);
    assert_eq!(summary.deleted, 3);

    // Pulizia: rimuove anche i backup superstiti per non sporcare i
    // container di test tra un'esecuzione e l'altra.
    let prefix = format!("{job_name}-");
    for name in dest.list().await.unwrap_or_default() {
        if name.starts_with(&prefix) {
            let _ = dest.delete(&name).await;
        }
    }
}

#[tokio::test]
async fn retention_prunes_old_backups_on_sftp() {
    if !is_reachable("127.0.0.1:2222") {
        eprintln!("SKIP: nessun server SFTP raggiungibile su 127.0.0.1:2222");
        return;
    }
    unsafe {
        std::env::set_var("BACKUPPER_SFTP_RETENTION_PASSWORD", "testpass");
    }
    assert_retention_prunes_old_backups("retention-sftp", || DestinationConfig::Sftp {
        host: "127.0.0.1".to_string(),
        port: 2222,
        user: "testuser".to_string(),
        password_env: Some("BACKUPPER_SFTP_RETENTION_PASSWORD".to_string()),
        key_path: None,
        key_passphrase_env: None,
        root: "/upload".to_string(),
        host_key_fingerprint: None,
        retry: RetryConfig::default(),
        bandwidth_limit_kib_s: None,
    })
    .await;
}

#[tokio::test]
async fn retention_prunes_old_backups_on_s3() {
    if !is_reachable("127.0.0.1:9000") {
        eprintln!("SKIP: nessun server S3 raggiungibile su 127.0.0.1:9000");
        return;
    }
    unsafe {
        std::env::set_var("BACKUPPER_S3_RETENTION_ACCESS_KEY", "minioadmin");
        std::env::set_var("BACKUPPER_S3_RETENTION_SECRET_KEY", "minioadmin");
    }
    assert_retention_prunes_old_backups("retention-s3", || DestinationConfig::S3 {
        bucket: "backuppo-test".to_string(),
        region: Some("us-east-1".to_string()),
        endpoint: Some("http://127.0.0.1:9000".to_string()),
        access_key_id_env: "BACKUPPER_S3_RETENTION_ACCESS_KEY".to_string(),
        secret_access_key_env: "BACKUPPER_S3_RETENTION_SECRET_KEY".to_string(),
        root: None,
        virtual_host_style: false,
        retry: RetryConfig::default(),
        bandwidth_limit_kib_s: None,
    })
    .await;
}

#[tokio::test]
async fn same_job_works_against_local_destination() {
    let src_dir = tempfile::tempdir().unwrap();
    let dst_dir = tempfile::tempdir().unwrap();
    write_file(&src_dir.path().join("a.txt"), "contenuto di test locale");

    let config = build_config(
        src_dir.path(),
        "local",
        DestinationConfig::Fs {
            root: dst_dir.path().to_string_lossy().to_string(),
        },
    );

    assert_job_round_trips(&config).await;
}

#[tokio::test]
async fn same_job_works_against_sftp_destination() {
    if !is_reachable("127.0.0.1:2222") {
        eprintln!("SKIP: nessun server SFTP raggiungibile su 127.0.0.1:2222 (vedi commento in cima al file)");
        return;
    }
    unsafe {
        std::env::set_var("BACKUPPER_SFTP_TEST_PASSWORD", "testpass");
    }

    let src_dir = tempfile::tempdir().unwrap();
    write_file(&src_dir.path().join("a.txt"), "contenuto di test sftp");

    let config = build_config(
        src_dir.path(),
        "sftp-test",
        DestinationConfig::Sftp {
            host: "127.0.0.1".to_string(),
            port: 2222,
            user: "testuser".to_string(),
            password_env: Some("BACKUPPER_SFTP_TEST_PASSWORD".to_string()),
            key_path: None,
            key_passphrase_env: None,
            root: "/upload".to_string(),
            host_key_fingerprint: None,
            retry: RetryConfig::default(),
            bandwidth_limit_kib_s: None,
        },
    );

    assert_job_round_trips(&config).await;
}

#[tokio::test]
async fn same_job_works_against_webdav_destination() {
    if !is_reachable("127.0.0.1:8080") {
        eprintln!("SKIP: nessun server WebDAV raggiungibile su 127.0.0.1:8080 (vedi commento in cima al file)");
        return;
    }
    unsafe {
        std::env::set_var("BACKUPPER_WEBDAV_TEST_PASSWORD", "testpass");
    }

    let src_dir = tempfile::tempdir().unwrap();
    write_file(&src_dir.path().join("a.txt"), "contenuto di test webdav");

    let config = build_config(
        src_dir.path(),
        "webdav-test",
        DestinationConfig::Webdav {
            url: "http://127.0.0.1:8080".to_string(),
            user: Some("testuser".to_string()),
            password_env: Some("BACKUPPER_WEBDAV_TEST_PASSWORD".to_string()),
            retry: RetryConfig::default(),
            bandwidth_limit_kib_s: None,
        },
    );

    assert_job_round_trips(&config).await;
}

#[tokio::test]
async fn same_job_works_against_s3_destination() {
    if !is_reachable("127.0.0.1:9000") {
        eprintln!("SKIP: nessun server S3 raggiungibile su 127.0.0.1:9000 (vedi commento in cima al file)");
        return;
    }
    unsafe {
        std::env::set_var("BACKUPPER_S3_TEST_ACCESS_KEY", "minioadmin");
        std::env::set_var("BACKUPPER_S3_TEST_SECRET_KEY", "minioadmin");
    }

    let src_dir = tempfile::tempdir().unwrap();
    write_file(&src_dir.path().join("a.txt"), "contenuto di test s3");

    let config = build_config(
        src_dir.path(),
        "s3-test",
        DestinationConfig::S3 {
            bucket: "backuppo-test".to_string(),
            region: Some("us-east-1".to_string()),
            endpoint: Some("http://127.0.0.1:9000".to_string()),
            access_key_id_env: "BACKUPPER_S3_TEST_ACCESS_KEY".to_string(),
            secret_access_key_env: "BACKUPPER_S3_TEST_SECRET_KEY".to_string(),
            root: None,
            virtual_host_style: false,
            retry: RetryConfig::default(),
            bandwidth_limit_kib_s: None,
        },
    );

    assert_job_round_trips(&config).await;
}
