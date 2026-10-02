#!/usr/bin/env bash
# Prepara una config agent effimera e avvia 'bkpo serve' per i test E2E
# Playwright. Invocato da webServer.command in playwright.config.ts: lo
# stato vive sotto apps/web/e2e/.state, rigenerato a ogni avvio.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
state="${root}/apps/web/e2e/.state"

rm -rf "${state}"
mkdir -p "${state}/source" "${state}/dest"
echo "contenuto di test e2e" > "${state}/source/hello.txt"

cat > "${state}/config.yaml" <<EOF
destinations:
  local:
    type: fs
    root: ${state}/dest
observability:
  history_path: ${state}/history.sqlite
  status_page: ${state}/status.html
api:
  bind: 127.0.0.1:18787
jobs: {}
EOF

cd "${root}"
npm run build --prefix apps/web
cargo build --locked --package backuppo
exec "${root}/target/debug/bkpo" serve --config "${state}/config.yaml"
