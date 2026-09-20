//! Crate `engine`: esecuzione job, retention, verifica restore.

pub mod archive;
mod runner;

pub use runner::run_job;
