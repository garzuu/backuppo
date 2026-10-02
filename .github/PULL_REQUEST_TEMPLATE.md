## Cosa cambia e perché

<!-- Una o due frasi. Se risolve una issue, linkala con "Closes #123". -->

## Checklist

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test --workspace`
- [ ] Se tocca il feature `hub`: `cargo clippy`/`cargo test` anche con `--features hub`
- [ ] Se tocca `apps/web/src`: `npm run build --prefix apps/web` e `npm run e2e --prefix apps/web`
- [ ] Documentazione aggiornata in `docs/src/content/docs/` (IT **e** EN) se il comportamento osservabile dall'utente è cambiato
- [ ] `CHANGELOG.md` aggiornato se la modifica è rilevante per chi usa Backuppo

## Note per chi revisiona

<!-- Decisioni non ovvie, cose su cui vuoi un parere, alternative scartate. -->
