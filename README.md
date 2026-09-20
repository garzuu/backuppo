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
```

Per l'installazione completa vedi il [quickstart](docs/quickstart.md). Sono
disponibili anche il [riferimento della configurazione](docs/configuration.md)
e la [guida al restore manuale](docs/restore.md).

## Comandi principali

```text
bkpo check       valida la configurazione
bkpo run         esegue un backup
bkpo verify      verifica l'ultimo backup
bkpo daemon      avvia scheduler, retention e report
bkpo serve       avvia soltanto API e Web UI locale
bkpo runs        mostra lo storico locale
bkpo logs <id>   mostra il log di un'esecuzione
bkpo notify-test prova i notifier configurati
```

## Licenza

Distribuito con doppia licenza [MIT](LICENSE-MIT) oppure
[Apache-2.0](LICENSE-APACHE).
