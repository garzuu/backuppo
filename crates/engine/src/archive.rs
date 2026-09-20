use std::fs::File;
use std::io::{self, Read, Write};
use std::path::Path;

use age::secrecy::SecretString;
use age::Identity;
use backuppo_core::error::BackupError;
use sha2::{Digest, Sha256};

/// Scrittore che, se una passphrase è presente, cifra tutto ciò che vi viene
/// scritto con `age` prima di passarlo al file sottostante.
enum Sink {
    Plain(File),
    Encrypted(age::stream::StreamWriter<File>),
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
    passphrase: Option<&str>,
) -> Result<(), BackupError> {
    let file = File::create(out_path)?;

    let sink = match passphrase {
        Some(pass) => {
            let encryptor =
                age::Encryptor::with_user_passphrase(SecretString::from(pass.to_string()));
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
    passphrase: Option<&str>,
) -> Result<(), BackupError> {
    let file = File::open(archive_path)?;

    let reader: Box<dyn Read> = match passphrase {
        Some(pass) => {
            let decryptor = age::Decryptor::new(file)
                .map_err(|e| BackupError::Other(format!("errore lettura archivio cifrato: {e}")))?;
            let identity = age::scrypt::Identity::new(SecretString::from(pass.to_string()));
            let identities: [&dyn Identity; 1] = [&identity];
            let reader = decryptor.decrypt(identities.into_iter()).map_err(|e| {
                BackupError::Other(format!("passphrase errata o archivio corrotto: {e}"))
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
