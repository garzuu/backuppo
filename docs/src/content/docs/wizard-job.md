---
title: Wizard e più job
description: Configurare uno o più job dall’interfaccia locale dell’Agent.
---

Apri `http://127.0.0.1:8787`. Se l’Agent gira su un server remoto, usa un
tunnel SSH:

```sh
ssh -L 8787:127.0.0.1:8787 utente@server
```

Apri quindi `http://127.0.0.1:8787` sul tuo computer e scegli
**Configurazione**. La barra laterale mostra i job presenti in questa
installazione; **Nuovo** ne aggiunge uno, senza sostituire quelli esistenti.

## I cinque passi

1. **Sorgente** — scegli cosa salvare e dove si trova.
2. **Destinazione** — seleziona una destinazione esistente oppure creane una.
3. **Pianificazione** — scegli una frequenza o inserisci un cron a cinque campi
   e imposta la retention.
4. **Protezione** — configura cifratura, compressione, verifica restore,
   soglia massima di età e i notifier (Telegram, email, webhook, ntfy): li
   crei, modifichi ed elimini direttamente qui, poi scegli quali usare per
   `on_success`/`on_failure`/`on_verify`. Il notifier `hub` resta gestito
   dalla pagina **Collegamento Hub**.
5. **Riepilogo** — controlla le scelte prima della validazione.

Le destinazioni sono risorse condivise: se ne modifichi una già usata, il
wizard indica quali altri job saranno interessati. Per partire da un job
simile usa **Duplica**, poi cambia soltanto sorgente o pianificazione.

## Bozza e configurazione attiva

**Salva bozza** aggiorna il file ma non il runtime. **Salva e applica** valida
il documento, crea `config.yaml.bak`, lo sostituisce atomicamente e ricarica
scheduler e job. Le esecuzioni già in corso non vengono interrotte. Una
modifica a `api.bind` richiede invece il riavvio del servizio.

I campi `*_env` contengono il **nome** della variabile d’ambiente, mai il
segreto. Il wizard non legge e non salva il valore della password o del token.
La vista YAML resta disponibile per le opzioni avanzate.

## Controllo finale

Dopo aver applicato la configurazione, avvia il job dalla UI oppure:

```sh
bkpo check --config /etc/backuppo/config.yaml
bkpo run --config /etc/backuppo/config.yaml --job nome-job
bkpo verify --config /etc/backuppo/config.yaml --job nome-job
```
