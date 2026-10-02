---
title: Modello di minaccia
description: Cosa protegge Backuppo, da chi, e dove si ferma la protezione.
---

Questa pagina elenca onestamente cosa Backuppo protegge e cosa no. Non è un
audit di sicurezza formale (nessuno è stato commissionato), ma il modo in
cui chi lo mantiene ragiona sulle scelte di design.

## Scenario 1 — l'host sorgente viene compromesso

Un attaccante ottiene l'esecuzione di codice sull'host che l'agent sta
proteggendo, con gli stessi privilegi del processo agent.

**Cosa può fare:** leggere i file sorgente del prossimo backup, leggere le
variabili d'ambiente risolte da `*_env` (password destinazione, passphrase
age, password Restic), scrivere backup corrotti o falsi sulla destinazione.

**Cosa non può fare, se configurato correttamente:**
- Cancellare o alterare i backup **già scritti**, se il repository Restic è
  `append_only: true` con credenziali di scrittura separate da quelle di
  manutenzione (vedi [sicurezza](../sicurezza/#limitazione-dei-privilegi)).
  Le credenziali di manutenzione (`maintenance_environment`) non sono mai
  caricate dal daemon: un attaccante che legge l'ambiente del processo non
  le trova.
- Cancellare backup su una destinazione S3 con Object Lock in modalità
  `compliance` entro il periodo di retention, verificabile con
  `bkpo storage-check`: nemmeno le credenziali amministrative del bucket lo
  permettono finché la retention non scade.
- Leggere i backup già scritti e cifrati senza la chiave di cifratura: se
  `encryption.key_env`/`passphrase_env` punta a una variabile non risolvibile
  dall'host compromesso (es. iniettata solo a runtime da un secret manager e
  non persistita), i dati storici restano protetti.

**Cosa non puoi evitare:** un host compromesso **prima** del prossimo backup
può far scrivere dati falsi o esfiltrare quelli nuovi. `verify_restore`
rileva corruzione e incoerenze strutturali, non la sottrazione silenziosa di
dati autentici né l'inserimento di dati falsi ma strutturalmente validi. Se
sospetti un host compromesso: revoca le credenziali di destinazione usate da
quell'agent, ruota le chiavi di cifratura (vedi
[rotazione delle chiavi](../sicurezza/#rotazione-delle-chiavi)) e tratta ogni
backup successivo alla compromissione come non affidabile finché non lo
verifichi a mano.

## Scenario 2 — solo la destinazione/storage viene compromessa

Un attaccante ottiene le credenziali amministrative del bucket S3, del
server SFTP o del provider WebDAV, ma non tocca l'agent.

**Cosa può fare:** leggere gli archivi cifrati (senza la chiave, non il
contenuto), cancellarli se non c'è Object Lock o se la modalità è
`governance` anziché `compliance`.

**Cosa non può fare:** decifrare gli archivi `age` o un repository Restic
senza la rispettiva chiave — quella non è mai nella destinazione.

## Scenario 3 — l'Hub viene compromesso

Un attaccante ottiene l'esecuzione di codice sull'host dell'Hub, o accesso
diretto al suo database.

**Cosa può fare:** leggere metadati di tutte le esecuzioni (nomi job, esiti,
durate, errori — mai contenuto dei backup, che l'Hub non riceve mai),
richiedere `run`/`verify` sugli agent con `remote_commands: true` abilitato
(mai comandi arbitrari), e — punto onestamente scomodo — **firmare policy
malevole** se ottiene anche `policy_signing_key_env`: la chiave di firma
vive sullo stesso host dell'Hub, non in un HSM separato. Un Hub
completamente compromesso può quindi, nella modalità `block`, sospendere lo
scheduler e la UI degli agent collegati (negazione di servizio), ma non può
far eseguire codice arbitrario né leggere contenuto dei backup.

**Mitigazione pratica:** tratta l'host dell'Hub con lo stesso livello di
cura di un host che gestisce segreti — accesso limitato, patching, nessun
altro servizio esposto sulla stessa macchina. Un token agent compromesso da
solo (senza compromettere l'Hub) permette solo eventi/heartbeat fittizi per
quel sito, mai accesso ad altri siti o alla Web UI.

## Scenario 4 — intercettazione di rete

**Protetto:** tutte le destinazioni remote (SFTP, S3, WebDAV, cloud drive)
usano TLS via `rustls`; SFTP supporta la verifica della host key
(`host_key_fingerprint`). Gli archivi sono già cifrati prima di lasciare
l'host, quindi anche un attaccante che intercetta il traffico in chiaro
(TLS compromesso o assente) ottiene solo dati cifrati — a meno che la
destinazione sia `fs` locale senza cifratura `age` attiva, nel qual caso i
dati non sono mai stati cifrati in primo luogo.

**Non protetto:** l'Hub parla HTTP in chiaro e **deve** stare dietro un
reverse proxy che termina TLS (vedi
[deploy dell'Hub](../hub-deploy/#reverse-proxy-con-tls)); esporlo senza
questo espone credenziali di login e token in chiaro.

## Scenario 5 — compromissione della supply chain di rilascio

Un attaccante ottiene `UPDATE_SIGNING_KEY_B64` (il secret CI usato per
firmare `update-stable.json`).

**Cosa può fare:** pubblicare un manifest di aggiornamento che `bkpo update`
accetterebbe come legittimo, se riesce anche a farlo scaricare dagli agent
(serve controllare `manifest_url` o compromettere la distribuzione).

**Mitigazione:** la chiave pubblica (`updates.public_key`) va distribuita
**indipendentemente** dal canale di release (non nello stesso repository
Git, idealmente) — vedi [aggiornamenti](../updates/). Se sospetti la
compromissione della chiave privata: revoca il secret CI, genera una nuova
coppia, distribuisci la nuova chiave pubblica fuori banda e comunica
l'incidente. Non esiste oggi un meccanismo di revoca automatica lato agent.

## Esplicitamente fuori scope

- Un host già compromesso con privilegi root/amministratore prima
  dell'installazione di Backuppo: chi ha quell'accesso può già leggere
  qualunque segreto il processo agent legge.
- Attacchi alla supply chain di dipendenze upstream (Rust crates, Restic,
  age): si affida agli avvisi RustSec e alle release verificate di quei
  progetti, non c'è auditing indipendente da parte di questo progetto.
- Accesso fisico alla macchina.
- Attacchi side-channel contro le implementazioni crittografiche usate
  (`age`, Restic): si assume che siano corrette; non sono state riverificate
  da questo progetto.

Per segnalare una vulnerabilità concreta (non uno scenario teorico), vedi
[SECURITY.md](https://github.com/garzuu/backuppo/blob/main/SECURITY.md).
