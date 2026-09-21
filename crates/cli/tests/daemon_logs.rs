use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

/// Regressione: senza `RUST_LOG` il daemon non scriveva nulla (filtro di
/// default = solo errori). Ora deve loggare a `info` su stderr, senza codici
/// colore quando l'output non è un terminale (file di log, journald).
#[test]
fn daemon_logs_at_info_without_rust_log_and_without_ansi_colors() {
    let temp = tempfile::tempdir().expect("tempdir");
    let config_path = temp.path().join("config.yaml");
    let root = temp.path().join("dest");
    let source = temp.path().join("src");
    std::fs::create_dir_all(&source).expect("source");
    let yaml_path = |path: &std::path::Path| {
        path.to_string_lossy()
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
    };
    std::fs::write(
        &config_path,
        format!(
            r#"
destinations:
  local:
    type: fs
    root: "{}"
jobs:
  documents:
    source:
      type: folder
      path: "{}"
    destination: local
    schedule: "0 3 * * *"
"#,
            yaml_path(&root),
            yaml_path(&source)
        ),
    )
    .expect("config");

    let mut child = Command::new(env!("CARGO_BIN_EXE_bkpo"))
        .args(["daemon", "--config"])
        .arg(&config_path)
        .env_remove("RUST_LOG")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("avvio daemon");

    // Legge stderr in un thread: il daemon non termina da solo.
    let stderr = child.stderr.take().expect("stderr");
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    // Raccoglie le righe fino a "daemon avviato" (o al timeout).
    let mut lines = Vec::new();
    while let Ok(line) = rx.recv_timeout(Duration::from_secs(20)) {
        let started = line.contains("daemon avviato");
        lines.push(line);
        if started {
            break;
        }
    }
    let _ = child.kill();
    let output = child.wait_with_output().expect("attesa");

    assert!(
        lines.iter().any(|line| line.contains("daemon avviato")),
        "nessun log \"daemon avviato\" su stderr entro 20 s: {lines:?}"
    );
    assert!(
        lines.iter().all(|line| !line.contains('\u{1b}')),
        "codici ANSI nei log: {lines:?}"
    );
    assert!(
        lines
            .iter()
            .all(|line| !line.contains("tokio_cron_scheduler")),
        "log di dipendenze a livello info nel default: {lines:?}"
    );
    assert!(
        output.stdout.is_empty(),
        "stdout deve restare libero dai log: {:?}",
        String::from_utf8_lossy(&output.stdout)
    );
}
