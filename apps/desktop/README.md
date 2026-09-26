# Backuppo Desktop

Wrapper Tauri opzionale della Web UI servita dall'agent o dall'hub. Non avvia
né incorpora `bkpo`: il servizio deve essere già attivo.

```sh
cd apps/desktop/src-tauri
cargo tauri dev -- --url=http://127.0.0.1:8787
cargo tauri dev -- --url=https://backup.example.com
```

Gli hub remoti devono usare HTTPS; HTTP è accettato soltanto su loopback.
