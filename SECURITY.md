# Politica di sicurezza

## Versioni supportate

Backuppo è pre-1.0: solo l'ultima release pubblicata riceve fix di
sicurezza. Non c'è ancora un impegno di backport su versioni precedenti.

## Segnalare una vulnerabilità

**Non aprire una issue pubblica.** Usa invece
[GitHub Security Advisories](https://github.com/garzuu/backuppo/security/advisories/new)
per questo repository, così la segnalazione resta privata finché non c'è un
fix pronto.

Se preferisci l'email, scrivi a **matteo@garzulano.com** includendo:

- versione di Backuppo (agent e/o hub) interessata;
- passi per riprodurre, o una descrizione del problema se non è
  riproducibile in modo affidabile;
- impatto che ritieni abbia (lettura/scrittura non autorizzata di backup,
  escalation di privilegi, bypass della verifica delle firme, ecc.).

Prova a rispondere entro una settimana. Non esiste un programma di bug
bounty.

## Cosa è "in scope"

- L'agent (`bkpo`) e l'hub (`backuppo-hub`), incluse le API che espongono.
- Il meccanismo di aggiornamento firmato (`bkpo update`) e la verifica delle
  policy firmate dall'hub.
- La Web UI incorporata in entrambi i binari.

## Cosa è esplicitamente fuori scope

- Problemi che richiedono accesso locale già privilegiato alla macchina su
  cui gira l'agent o l'hub (chi ha accesso root/amministratore all'host può
  già leggere la configurazione e le chiavi lì presenti: non è un problema
  di Backuppo).
- Repository Restic o destinazioni di terze parti (S3, SFTP, provider
  cloud): le relative vulnerabilità vanno segnalate al progetto o al
  fornitore competente.
- Vulnerabilità nelle dipendenze upstream già note e tracciate da
  `cargo audit`/RustSec: apri comunque una issue, ma non serve il canale
  privato se l'avviso è già pubblico.

## Buone pratiche già incorporate

Per contesto su cosa il progetto fa già (cifratura, superficie di rete,
privilegi, aggiornamenti firmati), vedi la pagina
[Sicurezza](https://garzuu.github.io/backuppo/sicurezza/) della
documentazione prima di segnalare qualcosa che potrebbe essere un
comportamento voluto piuttosto che un bug.
