---
title: Agent, Hub e collegamento
description: Quando usare l’Hub, come registrare un sito e come resta autonomo l’Agent.
---

Usa l’Hub quando devi osservare più installazioni, clienti o sedi da un unico
pannello. Per una singola macchina non è necessario: scegli **Stand-alone**
nella pagina **Collegamento Hub** dell’Agent.

## Cosa passa dall’Agent all’Hub

Passano heartbeat, nome del job, esito, durata, byte, checksum ed eventuale
errore. Non passano file, archivi, password delle sorgenti o credenziali delle
destinazioni.

## Registrare un sito

1. Nell’Hub crea o seleziona un **cliente** dalla lista.
2. Crea un **sito** sotto quel cliente. Il token Agent viene mostrato una sola
   volta: conservalo in un secret manager.
3. Nell’Agent apri **Collegamento Hub**, scegli **Collegato a un Hub** e segui
   il wizard Identità → Controllo → Conferma.
4. Inserisci l’URL HTTPS dell’Hub e il **nome** della variabile che contiene il
   token. Configura il token nell’ambiente del servizio e riavvia l’Agent.
5. Controlla che il sito passi a online entro la soglia configurata.

Il wizard crea il notifier `type: hub`, la coda locale e l’intervallo di
heartbeat. Tornare a **Stand-alone** rimuove il collegamento dalla
configurazione, senza eliminare job o backup.

## Comandi remoti e policy

I comandi remoti sono opt-in. Se abilitati, l’Agent interroga l’Hub e accetta
soltanto `run` e `verify` su job già presenti. Le richieste scadono dopo dieci
minuti e non possono eseguire shell arbitraria.

Le policy sono firmate Ed25519 e legate all’ID del sito. In modalità `audit`
le violazioni vengono registrate; in modalità `block` possono sospendere
scheduler, UI e comandi remoti finché la configurazione non torna conforme.

Per installare il servizio centrale prosegui con il [deploy dell’Hub](../hub-deploy/).
