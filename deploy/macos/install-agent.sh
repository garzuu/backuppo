#!/bin/sh
set -eu
[ "$(id -u)" -eq 0 ] || { echo "Eseguire con sudo" >&2; exit 1; }
SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
PAYLOAD_DIR=${BACKUPPO_PAYLOAD_DIR:-"$SCRIPT_DIR"}
DATA_DIR="/Library/Application Support/Backuppo"
LOG_DIR="/Library/Logs/Backuppo"
test -f "$PAYLOAD_DIR/bkpo" || { echo "bkpo non trovato" >&2; exit 1; }
mkdir -p "$DATA_DIR" "$LOG_DIR"
install -m 0755 "$PAYLOAD_DIR/bkpo" /usr/local/bin/bkpo
if [ -f "$PAYLOAD_DIR/restic" ]; then install -m 0755 "$PAYLOAD_DIR/restic" /usr/local/bin/restic; fi
if [ ! -f "$DATA_DIR/config.yaml" ]; then install -m 0600 "$PAYLOAD_DIR/config/agent-macos.yaml" "$DATA_DIR/config.yaml"; fi
install -m 0644 "$PAYLOAD_DIR/macos/com.backuppo.agent.plist" /Library/LaunchDaemons/com.backuppo.agent.plist
/usr/local/bin/bkpo check --config "$DATA_DIR/config.yaml"
launchctl bootout system/com.backuppo.agent 2>/dev/null || true
launchctl bootstrap system /Library/LaunchDaemons/com.backuppo.agent.plist
echo "Backuppo Agent installato. UI: http://127.0.0.1:8787"
