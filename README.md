# Backuppo

Backuppo (`bkpo`) è un agent di backup open source e cross-platform. Esegue
backup configurati in YAML verso filesystem, SFTP, S3, WebDAV e cloud drive, cifra gli
archivi con `age` e verifica automaticamente che siano davvero ripristinabili.

Le sorgenti supportate sono cartelle, PostgreSQL, MySQL/MariaDB, SQLite, volumi
Docker e comandi custom. L'agent funziona stand-alone, mantiene uno storico
SQLite locale, genera una pagina di stato e invia notifiche SMTP, Telegram o
webhook.

Per dataset grandi può delegare snapshot incrementali e deduplicati a Restic.
Il daemon espone inoltre una Web UI locale opzionale per stato, storico e
avvio manuale dei job.

## Avvio rapido

```sh
cp examples/config.yaml config.yaml
export BACKUP_PASSPHRASE='una-passphrase-lunga'
bkpo check --config config.yaml
bkpo run --config config.yaml --job home-documents
bkpo verify --config config.yaml --job home-documents
bkpo snapshots --config config.yaml --job home-documents
bkpo browse --config config.yaml --job home-documents --snapshot latest
bkpo restore --config config.yaml --job home-documents --snapshot latest --target ./restore
```

Agent e Hub sono distribuiti come prodotti distinti per Linux, macOS,
Windows e Docker. Per scegliere e installare il componente corretto vedi la
[guida di installazione](docs/src/content/docs/installazione.md); per il primo
backup continua poi con il [quickstart](docs/src/content/docs/quickstart.md).
Sono disponibili anche il [riferimento della configurazione](docs/src/content/docs/configuration.md)
e la [guida al restore manuale](docs/src/content/docs/restore.md).

## Comandi principali

```text
bkpo check       valida la configurazione
bkpo run         esegue un backup
bkpo verify      verifica l'ultimo backup
bkpo snapshots   elenca gli snapshot disponibili
bkpo browse      sfoglia il contenuto di uno snapshot
bkpo restore     ripristina in una directory sicura
bkpo storage-check verifica la policy S3 Object Lock
bkpo maintain    esegue retention Restic con credenziali amministrative
bkpo update      controlla e installa release firmate
bkpo daemon      avvia scheduler, retention e report
bkpo serve       avvia soltanto API e Web UI locale
bkpo runs        mostra lo storico locale
bkpo logs <id>   mostra il log di un'esecuzione
bkpo notify-test prova i notifier configurati
```

## Web UI

La stessa interfaccia responsive è incorporata nell'agent e nell'hub. Sul
singolo host permette di eseguire e verificare job, consultare storico e log
e modificare la configurazione con validazione e reload sicuro. Sull'hub
offre la vista multi-sito, i comandi remoti e l'amministrazione di utenti e
token. È installabile come PWA; è disponibile anche un wrapper Tauri
opzionale. Vedi la [guida alla UI](docs/ui.md).

## Licenza

Distribuito con doppia licenza [MIT](LICENSE-MIT) oppure
[Apache-2.0](LICENSE-APACHE).
