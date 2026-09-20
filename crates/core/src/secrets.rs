use crate::error::BackupError;

/// Risolve il valore di un segreto (password, token, ...) letto da una
/// variabile d'ambiente. `field` è il nome del campo di config (es.
/// `"password_env"`) usato solo per messaggi d'errore leggibili.
pub fn resolve_env(field: &str, var: &str) -> Result<String, BackupError> {
    std::env::var(var).map_err(|_| BackupError::MissingEnvVar {
        field: field.to_string(),
        var: var.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_existing_var() {
        // SAFETY: test eseguito in un singolo thread di test, nessuna race
        // su questa variabile.
        unsafe {
            std::env::set_var("BACKUPPER_TEST_SECRET", "hunter2");
        }
        let value = resolve_env("password_env", "BACKUPPER_TEST_SECRET").unwrap();
        assert_eq!(value, "hunter2");
        unsafe {
            std::env::remove_var("BACKUPPER_TEST_SECRET");
        }
    }

    #[test]
    fn errors_on_missing_var() {
        let err = resolve_env("token_env", "BACKUPPER_DOES_NOT_EXIST").unwrap_err();
        match err {
            BackupError::MissingEnvVar { field, var } => {
                assert_eq!(field, "token_env");
                assert_eq!(var, "BACKUPPER_DOES_NOT_EXIST");
            }
            other => panic!("expected MissingEnvVar, got {other:?}"),
        }
    }
}
