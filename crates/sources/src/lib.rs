//! Crate `sources`: implementazioni del trait `Source` di `core`.

mod command;
mod disk_image;
pub mod docker;
mod docker_volume;
mod folder;
mod libvirt_vm;
mod mysql;
mod postgres;
mod sqlite;

pub use command::CommandSource;
pub use disk_image::DiskImageSource;
pub use docker_volume::DockerVolumeSource;
pub use folder::FolderSource;
pub use libvirt_vm::LibvirtVmSource;
pub use mysql::MySqlSource;
pub use postgres::PostgresSource;
pub use sqlite::SqliteSource;

use backuppo_core::config::SourceConfig;
use backuppo_core::error::BackupError;
use backuppo_core::traits::Source;

/// Costruisce l'implementazione di `Source` corrispondente alla config.
pub fn build(config: &SourceConfig) -> Result<Box<dyn Source>, BackupError> {
    match config {
        SourceConfig::Folder { path, exclude } => Ok(Box::new(FolderSource::new(path, exclude)?)),
        SourceConfig::Postgres {
            host,
            port,
            user,
            password_env,
            database,
            container,
        } => Ok(Box::new(PostgresSource::new(
            host,
            *port,
            user,
            password_env,
            database,
            container.clone(),
        ))),
        SourceConfig::MySql {
            host,
            port,
            user,
            password_env,
            database,
            container,
        } => Ok(Box::new(MySqlSource::new(
            host,
            *port,
            user,
            password_env,
            database,
            container.clone(),
        ))),
        SourceConfig::Sqlite { path } => Ok(Box::new(SqliteSource::new(path))),
        SourceConfig::DockerVolume { volume } => Ok(Box::new(DockerVolumeSource::new(volume))),
        SourceConfig::Command {
            command,
            args,
            output_filename,
        } => Ok(Box::new(CommandSource::new(
            command,
            args.clone(),
            output_filename,
        )?)),
        SourceConfig::DiskImage {
            path,
            output_filename,
        } => Ok(Box::new(DiskImageSource::new(path, output_filename)?)),
        SourceConfig::LibvirtVm { name, virsh_binary } => {
            Ok(Box::new(LibvirtVmSource::new(name, virsh_binary)))
        }
    }
}
