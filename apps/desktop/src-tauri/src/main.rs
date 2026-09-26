use anyhow::{bail, Context, Result};
use tauri::{WebviewUrl, WebviewWindowBuilder};
use url::Url;

fn main() -> Result<()> {
    let target = target_url()?;
    tauri::Builder::default()
        .setup(move |app| {
            WebviewWindowBuilder::new(app, "main", WebviewUrl::External(target.clone()))
                .title("Backuppo")
                .inner_size(1280.0, 820.0)
                .min_inner_size(820.0, 600.0)
                .build()
                .context("creazione finestra Backuppo")?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .context("esecuzione client desktop")?;
    Ok(())
}

fn target_url() -> Result<Url> {
    let raw = std::env::args()
        .skip(1)
        .find_map(|argument| argument.strip_prefix("--url=").map(str::to_owned))
        .unwrap_or_else(|| "http://127.0.0.1:8787".to_string());
    let url = Url::parse(&raw).with_context(|| format!("URL Backuppo non valido: '{raw}'"))?;
    let local_http = url.scheme() == "http"
        && matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "::1"));
    if url.scheme() != "https" && !local_http {
        bail!("usare HTTPS per hub remoti; HTTP è consentito solo su loopback");
    }
    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_local_http_and_rejects_remote_http() {
        assert!(Url::parse("http://127.0.0.1:8787").is_ok());
        let remote = Url::parse("http://example.com").unwrap();
        let local_http = remote.scheme() == "http"
            && matches!(remote.host_str(), Some("127.0.0.1" | "localhost" | "::1"));
        assert!(!local_http);
    }
}
