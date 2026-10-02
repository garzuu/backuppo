# Contribuire a Backuppo

Grazie per l'interesse. Alcune regole pratiche prima di aprire una PR.

## Prima di scrivere codice

- Per bug piccoli e fix ovvi, una PR diretta va bene.
- Per cambi non banali (nuova sorgente/destinazione, modifiche al formato
  degli archivi, cambi all'API hub, qualunque cosa tocchi più crate), apri
  prima una issue per discutere l'approccio. Evita di investire ore in una
  PR che poi va ridisegnata.
- Le decisioni architetturali del progetto sono documentate nei commit e
  nella documentazione in `docs/`; se non è chiaro il perché di una scelta
  esistente, chiedi prima di cambiarla.

## Workspace e convenzioni

- Rust, edition 2021, workspace con più crate sotto `crates/`. `core` non
  dipende da nessun altro crate del workspace; tutti gli altri dipendono da
  `core`.
- Errori: `thiserror` nelle librerie, `anyhow` solo nel crate `cli`.
- Nessun `unwrap()`/`expect()` fuori dai test: ogni errore va propagato o
  loggato con contesto (quale job, quale fase, quale risorsa).
- Mai loggare segreti, token, password o contenuto dei backup.
- Ogni nuova sorgente/destinazione/notifier implementa il trait di `core` e
  ha almeno un test di integrazione.
- L'agent deve compilare, girare e passare tutti i test **senza** la
  feature Cargo `hub`. Il codice dell'hub sta dietro quel feature flag o in
  `crates/hub`; nessun crate dell'agent importa codice dell'hub.
- Frontend: `apps/web/` è una SPA React/TypeScript condivisa tra agent e
  hub, incorporata nei binari a build time (`include_str!`).

## Prima di aprire la PR

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy -p backuppo -p backuppo-notifiers -p backuppo-core --all-targets --features hub -- -D warnings
cargo test --workspace
cargo test -p backuppo -p backuppo-notifiers -p backuppo-core --features hub
npm run build --prefix apps/web
npm run e2e --prefix apps/web
```

Se la modifica tocca `apps/web/src`, esegui anche i test Playwright
(`apps/web/e2e/`) in locale prima di aprire la PR: la CI li esegue ma sono
più lenti di un feedback locale.

## Documentazione

Se cambi un comportamento osservabile dall'utente (comando CLI, campo di
configurazione, endpoint API, voce del wizard), aggiorna anche la pagina
corrispondente in `docs/src/content/docs/` — sia la versione italiana che
quella inglese in `docs/src/content/docs/en/`. Una PR che cambia
comportamento senza toccare la documentazione viene probabilmente rimandata
indietro.

## Sicurezza

Per vulnerabilità, non aprire una issue pubblica: vedi
[`SECURITY.md`](SECURITY.md).

## Licenza

I contributi sono accettati con doppia licenza MIT/Apache-2.0, come il resto
del progetto (vedi [`LICENSE-MIT`](LICENSE-MIT) e
[`LICENSE-APACHE`](LICENSE-APACHE)).
