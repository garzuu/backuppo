//! Crate `core`: trait, tipi, errori e configurazione condivisi da tutto il
//! workspace. Nessuna dipendenza interna verso altri crate del workspace.

pub mod config;
pub mod error;
pub mod hub_protocol;
pub mod model;
pub mod secrets;
pub mod traits;

pub use error::BackupError;
pub use model::{Artifact, JobEvent};
pub use traits::{Destination, Notifier, Source};
