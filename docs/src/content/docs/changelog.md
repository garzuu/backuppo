---
title: Changelog e roadmap pubblica
description: Cosa è cambiato di recente e cosa è pianificato per Backuppo.
---

Lo storico dettagliato delle modifiche è in
[`CHANGELOG.md`](https://github.com/garzuu/backuppo/blob/main/CHANGELOG.md)
nel repository, aggiornato a ogni release. Questa pagina riassume la
direzione del progetto: cosa è già disponibile e cosa è ancora da fare, senza
promettere date.

## Disponibile oggi

- Agent stand-alone con verifica automatica del restore, motore Restic
  opzionale, cifratura `age`, destinazioni locali/SFTP/S3/WebDAV/cloud drive.
- Hub multi-sito opzionale, comandi remoti limitati, policy di sicurezza
  firmate.
- Web UI incorporata (wizard multi-job, storico, restore selettivo), PWA,
  wrapper desktop Tauri opzionale.
- Binari firmati per Linux/macOS/Windows, immagine Docker su GHCR,
  pacchetti `.deb`/`.rpm` automatici a ogni release. Template per Homebrew,
  AUR e winget pronti (pubblicazione manuale, vedi
  [pacchetti di sistema](../packaging/)).
- Modalità Restic append-only, verifica S3 Object Lock, aggiornamenti
  firmati Ed25519 con rollback.

## In lavorazione o pianificato

Nessun impegno su tempistiche: l'ordine riflette priorità attuali, non una
sequenza garantita.

- **Pubblicazione pacchetti esterni**: aprire le PR su un tap Homebrew
  personale, AUR e `winget-pkgs` per le prossime release (i file sono già
  generati dallo script di rendering, manca solo il passo di pubblicazione).
- **Sorgenti VM aggiuntive**: oggi solo libvirt/KVM è supportato (VM spenta).
  VMware (vSphere/ESXi) e Proxmox (PVE) sono candidati ma non ancora
  implementati.
- **Restore assistito interattivo**: un flusso guidato (`bkpo restore`
  interattivo, scelta di job/data/destinazione, restore di database
  direttamente in un container) oltre a quanto già disponibile da CLI/API/UI.
- **Integrazioni**: endpoint metriche Prometheus, integrazione Home
  Assistant, un server MCP in sola lettura per interrogare lo stato dei
  backup.
- **Sito**: tabella di confronto ([già pubblicata](../confronto/)) e
  controllo automatico dei link rotti in CI sono già attivi.

## Segnalare un problema o proporre qualcosa

Le issue e le discussioni vivono su
[GitHub](https://github.com/garzuu/backuppo). Per modifiche non banali,
apri prima una issue: vedi
[CONTRIBUTING.md](https://github.com/garzuu/backuppo/blob/main/CONTRIBUTING.md)
per le convenzioni del progetto. Per vulnerabilità di sicurezza segui
invece [SECURITY.md](https://github.com/garzuu/backuppo/blob/main/SECURITY.md)
e non aprire una issue pubblica.
