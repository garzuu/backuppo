//! Crate `engine`: esecuzione job, retention, verifica restore.

pub mod archive;
mod manifest;
mod runner;
mod verify;

pub use manifest::MANIFEST_FILENAME;
pub use runner::run_job;
pub use verify::verify_job;
