#!/bin/sh
set -eu
[ "$(id -u)" -eq 0 ] || { echo "Eseguire con sudo" >&2; exit 1; }
SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
PAYLOAD_DIR=${BACKUPPO_PAYLOAD_DIR:-"$SCRIPT_DIR"}
DATA_DIR="/Library/Application Support/Backuppo Hub"
LOG_DIR="/Library/Logs/Backuppo Hub"
PLIST=/Library/LaunchDaemons/com.backuppo.hub.plist
test -f "$PAYLOAD_DIR/backuppo-hub" || { echo "backuppo-hub non trovato" >&2; exit 1; }
mkdir -p "$DATA_DIR" "$LOG_DIR"
install -m 0755 "$PAYLOAD_DIR/backuppo-hub" /usr/local/bin/backuppo-hub
if [ ! -f "$DATA_DIR/config.yaml" ]; then install -m 0600 "$PAYLOAD_DIR/config/hub-macos.yaml" "$DATA_DIR/config.yaml"; fi
if [ ! -f "$PLIST" ]; then
  install -m 0600 "$PAYLOAD_DIR/macos/com.backuppo.hub.plist" "$PLIST"
  JWT_SECRET=$(openssl rand -hex 32)
  POLICY_KEY=$(openssl rand -base64 32 | tr -d '\n')
  /usr/libexec/PlistBuddy -c "Add :EnvironmentVariables dict" "$PLIST"
  /usr/libexec/PlistBuddy -c "Add :EnvironmentVariables:HUB_JWT_SECRET string $JWT_SECRET" "$PLIST"
  /usr/libexec/PlistBuddy -c "Add :EnvironmentVariables:HUB_POLICY_SIGNING_KEY string $POLICY_KEY" "$PLIST"
fi
/usr/local/bin/backuppo-hub check --config "$DATA_DIR/config.yaml"
launchctl bootout system/com.backuppo.hub 2>/dev/null || true
launchctl bootstrap system "$PLIST"
echo "Backuppo Hub installato su http://127.0.0.1:8080"
