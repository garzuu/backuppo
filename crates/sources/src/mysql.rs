use std::path::Path;
use std::time::Duration;

use async_trait::async_trait;
use backuppo_core::error::BackupError;
use backuppo_core::model::Artifact;
use backuppo_core::secrets::resolve_env;
use backuppo_core::traits::Source;
use tokio::process::Command;

use crate::docker;

const DUMP_FILENAME: &str = "dump.sql";
const VERIFY_IMAGE: &str = "mysql:8.4";
const VERIFY_PASSWORD: &str = "backuppo-restore-check";
const VERIFY_DATABASE: &str = "backuppo_verify";

pub struct MySqlSource {
    host: String,
    port: u16,
    user: String,
    password_env: String,
    database: String,
    container: Option<String>,
}

impl MySqlSource {
    pub fn new(
        host: impl Into<String>,
        port: u16,
        user: impl Into<String>,
        password_env: impl Into<String>,
        database: impl Into<String>,
        container: Option<String>,
    ) -> Self {
        Self {
            host: host.into(),
            port,
            user: user.into(),
            password_env: password_env.into(),
            database: database.into(),
            container,
        }
    }

    async fn dump(&self, password: &str) -> Result<Vec<u8>, BackupError> {
        let args = vec![
            "--host".to_string(),
            self.host.clone(),
            "--port".to_string(),
            self.port.to_string(),
            "--user".to_string(),
            self.user.clone(),
            "--single-transaction".to_string(),
            "--quick".to_string(),
            "--routines".to_string(),
            "--events".to_string(),
            "--triggers".to_string(),
            self.database.clone(),
        ];

        if let Some(container) = &self.container {
            let client = docker::connect()?;
            let mut cmd = vec!["mysqldump".to_string()];
            cmd.extend(args);
            let result = docker::exec(
                &client,
                container,
                cmd,
                Some(vec![format!("MYSQL_PWD={password}")]),
                None,
            )
            .await?;
            if result.exit_code != 0 {
                return Err(command_failed(
                    "mysqldump",
                    result.exit_code,
                    &result.stderr,
                ));
            }
            return Ok(result.stdout);
        }

        let output = Command::new("mysqldump")
            .args(args)
            .env("MYSQL_PWD", password)
            .output()
            .await
            .map_err(|e| BackupError::Other(format!("impossibile eseguire mysqldump: {e}")))?;
        if !output.status.success() {
            return Err(BackupError::Other(format!(
                "mysqldump è fallito con {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            )));
        }
        Ok(output.stdout)
    }
}

#[async_trait]
impl Source for MySqlSource {
    async fn prepare(&self, staging: &Path) -> Result<Artifact, BackupError> {
        let password = resolve_env("source.password_env", &self.password_env)?;
        let dump = self.dump(&password).await?;
        tokio::fs::create_dir_all(staging).await?;
        tokio::fs::write(staging.join(DUMP_FILENAME), &dump).await?;
        Ok(Artifact {
            path: staging.to_path_buf(),
            bytes: dump.len() as u64,
            files: 1,
            checksum: String::new(),
        })
    }

    async fn cleanup(&self, artifact: &Artifact) -> Result<(), BackupError> {
        if tokio::fs::metadata(&artifact.path).await.is_ok() {
            tokio::fs::remove_dir_all(&artifact.path).await?;
        }
        Ok(())
    }

    async fn verify_restore(&self, restored_dir: &Path) -> Result<(), BackupError> {
        let dump = tokio::fs::read(restored_dir.join(DUMP_FILENAME)).await?;
        verify_dump(&dump).await
    }
}

async fn verify_dump(dump: &[u8]) -> Result<(), BackupError> {
    let client = docker::connect()?;
    let name = docker::unique_container_name("verify-mysql");
    docker::run_throwaway_container(
        &client,
        &name,
        VERIFY_IMAGE,
        vec![
            format!("MYSQL_ROOT_PASSWORD={VERIFY_PASSWORD}"),
            format!("MYSQL_DATABASE={VERIFY_DATABASE}"),
        ],
        None,
        None,
    )
    .await?;

    let result = async {
        docker::wait_ready(60, Duration::from_secs(1), || async {
            let probe = docker::exec(
                &client,
                &name,
                vec!["mysqladmin".into(), "ping".into(), "--silent".into()],
                Some(vec![format!("MYSQL_PWD={VERIFY_PASSWORD}")]),
                None,
            )
            .await?;
            Ok(probe.exit_code == 0)
        })
        .await?;

        let restored = docker::exec(
            &client,
            &name,
            vec!["mysql".into(), "--user=root".into(), VERIFY_DATABASE.into()],
            Some(vec![format!("MYSQL_PWD={VERIFY_PASSWORD}")]),
            Some(dump),
        )
        .await?;
        if restored.exit_code != 0 {
            return Err(command_failed(
                "mysql restore",
                restored.exit_code,
                &restored.stderr,
            ));
        }

        let query = docker::exec(
            &client,
            &name,
            vec![
                "mysql".into(),
                "--user=root".into(),
                "--batch".into(),
                "--skip-column-names".into(),
                "--execute=SELECT 1".into(),
                VERIFY_DATABASE.into(),
            ],
            Some(vec![format!("MYSQL_PWD={VERIFY_PASSWORD}")]),
            None,
        )
        .await?;
        if query.exit_code != 0 || String::from_utf8_lossy(&query.stdout).trim() != "1" {
            return Err(command_failed(
                "mysql query",
                query.exit_code,
                &query.stderr,
            ));
        }
        Ok(())
    }
    .await;

    docker::remove_container(&client, &name).await;
    result
}

fn command_failed(command: &str, exit_code: i64, stderr: &[u8]) -> BackupError {
    BackupError::Other(format!(
        "{command} è fallito (exit code {exit_code}): {}",
        String::from_utf8_lossy(stderr)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "richiede un demone Docker locale e può scaricare mysql:8.4"]
    async fn dump_is_restorable_in_a_throwaway_container() {
        let client = docker::connect().expect("Docker");
        let container = docker::unique_container_name("test-mysql-source");
        docker::run_throwaway_container(
            &client,
            &container,
            VERIFY_IMAGE,
            vec![
                format!("MYSQL_ROOT_PASSWORD={VERIFY_PASSWORD}"),
                format!("MYSQL_DATABASE={VERIFY_DATABASE}"),
            ],
            None,
            None,
        )
        .await
        .expect("container sorgente");

        let result = async {
            docker::wait_ready(60, Duration::from_secs(1), || async {
                let probe = docker::exec(
                    &client,
                    &container,
                    vec!["mysqladmin".into(), "ping".into(), "--silent".into()],
                    Some(vec![format!("MYSQL_PWD={VERIFY_PASSWORD}")]),
                    None,
                )
                .await?;
                Ok(probe.exit_code == 0)
            })
            .await?;
            let seed = docker::exec(
                &client,
                &container,
                vec![
                    "mysql".into(),
                    "--user=root".into(),
                    "--execute=CREATE TABLE backuppo_test(id integer); INSERT INTO backuppo_test VALUES (42);".into(),
                    VERIFY_DATABASE.into(),
                ],
                Some(vec![format!("MYSQL_PWD={VERIFY_PASSWORD}")]),
                None,
            )
            .await?;
            if seed.exit_code != 0 {
                return Err(command_failed("seed mysql", seed.exit_code, &seed.stderr));
            }

            let variable = "BACKUPPO_TEST_MYSQL_PASSWORD";
            unsafe { std::env::set_var(variable, VERIFY_PASSWORD) };
            let source = MySqlSource::new(
                "localhost",
                3306,
                "root",
                variable,
                VERIFY_DATABASE,
                Some(container.clone()),
            );
            let staging = tempfile::tempdir()?;
            let data = staging.path().join("data");
            let artifact = source.prepare(&data).await?;
            assert_eq!(artifact.files, 1);
            source.verify_restore(&data).await?;
            unsafe { std::env::remove_var(variable) };
            Ok::<(), BackupError>(())
        }
        .await;

        docker::remove_container(&client, &container).await;
        result.expect("round trip MySQL");
    }
}
