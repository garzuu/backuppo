use std::fs;
use std::process::Command;

#[test]
fn runs_and_logs_show_the_persisted_execution() {
    let temp = tempfile::tempdir().expect("tempdir");
    let source = temp.path().join("source");
    let destination = temp.path().join("destination");
    let history = temp.path().join("state/history.sqlite");
    let status = temp.path().join("state/status.html");
    fs::create_dir_all(&source).expect("source");
    fs::write(source.join("file.txt"), "cli history").expect("file");
    let config_path = temp.path().join("config.yaml");
    let yaml_path = |path: &std::path::Path| {
        path.to_string_lossy()
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
    };
    let yaml = format!(
        r#"
destinations:
  local:
    type: fs
    root: "{}"
observability:
  history_path: "{}"
  status_page: "{}"
jobs:
  documents:
    source:
      type: folder
      path: "{}"
    destination: local
    compression: none
    schedule: "0 3 * * *"
"#,
        yaml_path(&destination),
        yaml_path(&history),
        yaml_path(&status),
        yaml_path(&source)
    );
    fs::write(&config_path, yaml).expect("config");

    let binary = env!("CARGO_BIN_EXE_bkpo");
    let run = Command::new(binary)
        .args([
            "run",
            "--config",
            config_path.to_str().unwrap(),
            "--job",
            "documents",
        ])
        .output()
        .expect("bkpo run");
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );

    let runs = Command::new(binary)
        .args(["runs", "--config", config_path.to_str().unwrap()])
        .output()
        .expect("bkpo runs");
    assert!(runs.status.success());
    let output = String::from_utf8(runs.stdout).unwrap();
    assert!(output.contains("documents"));
    assert!(output.contains("success"));

    let logs = Command::new(binary)
        .args(["logs", "--config", config_path.to_str().unwrap(), "1"])
        .output()
        .expect("bkpo logs");
    assert!(logs.status.success());
    let output = String::from_utf8(logs.stdout).unwrap();
    assert!(output.contains("sorgente preparata"));
    assert!(output.contains("upload confermato"));
    assert!(status.exists());
}
