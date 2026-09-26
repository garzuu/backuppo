#!/bin/sh
set -eu

if [ "$(id -u)" -ne 0 ]; then
  echo "Eseguire come root: sudo ./install-agent.sh" >&2
  exit 1
fi

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
PAYLOAD_DIR=${BACKUPPO_PAYLOAD_DIR:-"$SCRIPT_DIR"}
BINARY=${BACKUPPO_AGENT_BINARY:-"$PAYLOAD_DIR/bkpo"}
RESTIC=${BACKUPPO_RESTIC_BINARY:-"$PAYLOAD_DIR/restic"}
CONFIG_TEMPLATE=${BACKUPPO_AGENT_CONFIG:-"$PAYLOAD_DIR/config/agent.yaml"}
SERVICE_TEMPLATE=${BACKUPPO_AGENT_SERVICE:-"$PAYLOAD_DIR/systemd/backuppo.service"}

test -f "$BINARY" || { echo "bkpo non trovato: $BINARY" >&2; exit 1; }
test -f "$CONFIG_TEMPLATE" || { echo "configurazione iniziale non trovata: $CONFIG_TEMPLATE" >&2; exit 1; }
test -f "$SERVICE_TEMPLATE" || { echo "unit systemd non trovata: $SERVICE_TEMPLATE" >&2; exit 1; }
command -v systemctl >/dev/null 2>&1 || { echo "systemd non disponibile" >&2; exit 1; }

if ! getent group backuppo >/dev/null 2>&1; then groupadd --system backuppo; fi
if ! id backuppo >/dev/null 2>&1; then useradd --system --gid backuppo --home-dir /var/lib/backuppo --shell /usr/sbin/nologin backuppo; fi

install -d -m 0750 -o backuppo -g backuppo /var/lib/backuppo /var/backups/backuppo
install -d -m 0770 -o root -g backuppo /etc/backuppo
install -m 0755 "$BINARY" /usr/local/bin/bkpo
if [ -f "$RESTIC" ]; then install -m 0755 "$RESTIC" /usr/local/bin/restic; fi
if [ ! -f /etc/backuppo/config.yaml ]; then install -m 0640 -o backuppo -g backuppo "$CONFIG_TEMPLATE" /etc/backuppo/config.yaml; fi
if [ ! -f /etc/backuppo/backuppo.env ]; then install -m 0640 -o root -g backuppo /dev/null /etc/backuppo/backuppo.env; fi
install -m 0644 "$SERVICE_TEMPLATE" /etc/systemd/system/backuppo.service
/usr/local/bin/bkpo check --config /etc/backuppo/config.yaml

systemctl daemon-reload
systemctl enable backuppo.service
systemctl restart backuppo.service
echo "Backuppo Agent installato. UI: http://127.0.0.1:8787"
echo "Config: /etc/backuppo/config.yaml  Segreti: /etc/backuppo/backuppo.env"
