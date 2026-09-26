use std::fs::File;
use std::io::{self, Read, Write};
use std::path::Path;

use age::secrecy::SecretString;
use age::{Identity, Recipient};
use backuppo_core::error::BackupError;
use sha2::{Digest, Sha256};

/// Scrittore che, se una passphrase è presente, cifra tutto ciò che vi viene
/// scritto con `age` prima di passarlo al file sottostante.
enum Sink {
    Plain(File),
    Encrypted(age::stream::StreamWriter<File>),
}

#[derive(Debug, Clone)]
pub enum EncryptionMaterial {
    Passphrase(String),
    X25519Identity(String),
}

impl Write for Sink {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Sink::Plain(f) => f.write(buf),
            Sink::Encrypted(w) => w.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Sink::Plain(f) => f.flush(),
            Sink::Encrypted(w) => w.flush(),
        }
    }
}

impl Sink {
    fn finish(self) -> io::Result<()> {
        match self {
            Sink::Plain(mut f) => f.flush(),
            Sink::Encrypted(w) => w.finish().map(|_| ()),
        }
    }
}

/// Crea l'archivio del contenuto di `staging` in `out_path`: tar, poi
/// (opzionalmente) compressione zstd, poi (opzionalmente) cifratura age con
/// passphrase. Esegue I/O bloccante: va chiamata da un thread dedicato
/// (`spawn_blocking`) quando usata in contesto async.
pub fn build_archive(
    staging: &Path,
    out_path: &Path,
    compress: bool,
    encryption: Option<&EncryptionMaterial>,
) -> Result<(), BackupError> {
    let file = File::create(out_path)?;

    let sink = match encryption {
        Some(EncryptionMaterial::Passphrase(pass)) => {
            let encryptor =
                age::Encryptor::with_user_passphrase(SecretString::from(pass.to_string()));
            let writer = encryptor
                .wrap_output(file)
                .map_err(|e| BackupError::Other(format!("errore cifratura age: {e}")))?;
            Sink::Encrypted(writer)
        }
        Some(EncryptionMaterial::X25519Identity(encoded)) => {
            let identity: age::x25519::Identity = encoded.trim().parse().map_err(|error| {
                BackupError::Other(format!("chiave privata age non valida: {error}"))
            })?;
            let recipient = identity.to_public();
            let recipients: [&dyn Recipient; 1] = [&recipient];
            let encryptor =
                age::Encryptor::with_recipients(recipients.into_iter()).map_err(|error| {
                    BackupError::Other(format!("recipient age non valido: {error}"))
                })?;
            let writer = encryptor
                .wrap_output(file)
                .map_err(|e| BackupError::Other(format!("errore cifratura age: {e}")))?;
            Sink::Encrypted(writer)
        }
        None => Sink::Plain(file),
    };

    if compress {
        let mut encoder = zstd::stream::write::Encoder::new(sink, 0)?;
        write_tar(staging, &mut encoder)?;
        let sink = encoder.finish()?;
        sink.finish()?;
    } else {
        let mut sink = sink;
        write_tar(staging, &mut sink)?;
        sink.finish()?;
    }

    Ok(())
}

fn write_tar<W: Write>(staging: &Path, writer: W) -> Result<(), BackupError> {
    let mut builder = tar::Builder::new(writer);
    builder.append_dir_all(".", staging)?;
    builder.finish()?;
    Ok(())
}

/// Estrae un archivio prodotto da [`build_archive`] in `dest`, invertendo
/// cifratura e compressione. I/O bloccante: va chiamata via `spawn_blocking`
/// in contesto async.
pub fn extract_archive(
    archive_path: &Path,
    dest: &Path,
    compressed: bool,
    encryption: Option<&EncryptionMaterial>,
) -> Result<(), BackupError> {
    let file = File::open(archive_path)?;

    let identity: Option<Box<dyn Identity>> = match encryption {
        Some(EncryptionMaterial::Passphrase(pass)) => Some(Box::new(age::scrypt::Identity::new(
            SecretString::from(pass.to_string()),
        ))),
        Some(EncryptionMaterial::X25519Identity(encoded)) => {
            let identity: age::x25519::Identity = encoded.trim().parse().map_err(|error| {
                BackupError::Other(format!("chiave privata age non valida: {error}"))
            })?;
            Some(Box::new(identity))
        }
        None => None,
    };
    let reader: Box<dyn Read> = match identity.as_ref() {
        Some(identity) => {
            let decryptor = age::Decryptor::new(file)
                .map_err(|e| BackupError::Other(format!("errore lettura archivio cifrato: {e}")))?;
            let identities: [&dyn Identity; 1] = [identity.as_ref()];
            let reader = decryptor.decrypt(identities.into_iter()).map_err(|e| {
                BackupError::Other(format!("identita' errata o archivio corrotto: {e}"))
            })?;
            Box::new(reader)
        }
        None => Box::new(file),
    };

    if compressed {
        let decoder = zstd::stream::read::Decoder::new(reader)?;
        tar::Archive::new(decoder).unpack(dest)?;
    } else {
        tar::Archive::new(reader).unpack(dest)?;
    }

    Ok(())
}

/// Calcola lo SHA-256 di un file, in formato hex. I/O bloccante.
pub fn sha256_file(path: &Path) -> Result<String, BackupError> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let digest = hasher.finalize();
    Ok(digest.iter().map(|b| format!("{b:02x}")).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use age::secrecy::ExposeSecret;

    #[test]
    fn x25519_identity_round_trips() {
        let source = tempfile::tempdir().unwrap();
        let restored = tempfile::tempdir().unwrap();
        let output = tempfile::tempdir().unwrap();
        let archive = output.path().join("backup.tar.age");
        std::fs::write(source.path().join("data.txt"), b"secret").unwrap();
        let identity = age::x25519::Identity::generate();
        let material =
            EncryptionMaterial::X25519Identity(identity.to_string().expose_secret().to_string());
        build_archive(source.path(), &archive, false, Some(&material)).unwrap();
        extract_archive(&archive, restored.path(), false, Some(&material)).unwrap();
        assert_eq!(
            std::fs::read(restored.path().join("data.txt")).unwrap(),
            b"secret"
        );
    }
}
