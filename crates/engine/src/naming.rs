//! Nomi dei backup seguono lo schema `<job>-<timestamp-unix>.tar...`
//! (vedi `runner::run_job_impl`): questo modulo centralizza la logica per
//! leggerne il timestamp, condivisa da `verify` e `retention`.

/// Estrae il timestamp unix dal nome file `<job>-<timestamp>.tar...`.
pub(crate) fn extract_timestamp(name: &str) -> Option<u64> {
    let (_, after) = name.rsplit_once('-')?;
    let ts = after.split('.').next()?;
    ts.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_timestamp_from_filename() {
        assert_eq!(
            extract_timestamp("home-documents-1732104000.tar.zst.age"),
            Some(1732104000)
        );
        assert_eq!(extract_timestamp("no-timestamp-here"), None);
    }
}
