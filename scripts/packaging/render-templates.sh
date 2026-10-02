#!/usr/bin/env bash
# Riempie i template in deploy/packaging/{homebrew,aur,winget} con versione
# e checksum presi da SHA256SUMS di una release GitHub già pubblicata.
#
# Uso: scripts/packaging/render-templates.sh 0.2.0
#
# Output in dist/packaging/<versione>/. Da qui i file vanno copiati a mano
# nei rispettivi repo esterni (tap Homebrew, AUR, winget-pkgs): questo
# script NON pubblica nulla, prepara solo i contenuti.
set -euo pipefail

version="${1:?uso: render-templates.sh <versione, es. 0.2.0>}"
repo="garzuu/backuppo"
tag="v${version}"
out="dist/packaging/${version}"
mkdir -p "${out}"

sums_url="https://github.com/${repo}/releases/download/${tag}/SHA256SUMS"
sums_file="$(mktemp)"
trap 'rm -f "${sums_file}"' EXIT
echo "Scarico ${sums_url}"
curl --fail --location --silent --show-error "${sums_url}" -o "${sums_file}"

sha_for() {
  local filename="$1"
  grep "  ${filename}\$" "${sums_file}" | cut -d' ' -f1 \
    || { echo "checksum non trovato per ${filename}" >&2; exit 1; }
}

sha_macos_arm64="$(sha_for "backuppo-agent-${tag}-aarch64-apple-darwin.tar.gz")"
sha_macos_x86_64="$(sha_for "backuppo-agent-${tag}-x86_64-apple-darwin.tar.gz")"
sha_linux_x86_64="$(sha_for "backuppo-agent-${tag}-x86_64-unknown-linux-musl.tar.gz")"
sha_windows_x86_64="$(sha_for "backuppo-agent-${tag}-x86_64-pc-windows-msvc.zip")"

render() {
  local template="$1" destination="$2"
  sed \
    -e "s/__VERSION__/${version}/g" \
    -e "s/__SHA256_MACOS_ARM64__/${sha_macos_arm64}/g" \
    -e "s/__SHA256_MACOS_X86_64__/${sha_macos_x86_64}/g" \
    -e "s/__SHA256_LINUX_X86_64__/${sha_linux_x86_64}/g" \
    -e "s/__SHA256_WINDOWS_X86_64__/${sha_windows_x86_64}/g" \
    "${template}" > "${destination}"
  echo "scritto ${destination}"
}

mkdir -p "${out}/homebrew" "${out}/aur" "${out}/winget"
render deploy/packaging/homebrew/backuppo.rb.tmpl "${out}/homebrew/backuppo.rb"
render deploy/packaging/aur/PKGBUILD.tmpl "${out}/aur/PKGBUILD"
render deploy/packaging/winget/garzuu.Backuppo.yaml.tmpl \
  "${out}/winget/garzuu.Backuppo.yaml"
render deploy/packaging/winget/garzuu.Backuppo.installer.yaml.tmpl \
  "${out}/winget/garzuu.Backuppo.installer.yaml"
render deploy/packaging/winget/garzuu.Backuppo.locale.en-US.yaml.tmpl \
  "${out}/winget/garzuu.Backuppo.locale.en-US.yaml"

cat <<EOF

Fatto. Prossimi passi manuali (fuori da questo repo):
  - Homebrew: copia ${out}/homebrew/backuppo.rb nel tap garzuu/homebrew-backuppo
  - AUR: copia ${out}/aur/PKGBUILD in un checkout di aur.archlinux.org/backuppo-bin.git, poi 'makepkg --printsrcinfo > .SRCINFO' e push
  - winget: apri una PR su microsoft/winget-pkgs con i 3 file in ${out}/winget/
    nel percorso manifests/g/garzuu/Backuppo/${version}/
EOF
