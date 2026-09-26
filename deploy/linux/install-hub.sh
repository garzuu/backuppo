#!/bin/sh
set -eu

if [ "$(id -u)" -ne 0 ]; then
  echo "Eseguire come root: sudo ./install-hub.sh" >&2
  exit 1
fi

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
PAYLOAD_DIR=${BACKUPPO_PAYLOAD_DIR:-"$SCRIPT_DIR"}
BINARY=${BACKUPPO_HUB_BINARY:-"$PAYLOAD_DIR/backuppo-hub"}
CONFIG_TEMPLATE=${BACKUPPO_HUB_CONFIG:-"$PAYLOAD_DIR/config/hub.yaml"}
SERVICE_TEMPLATE=${BACKUPPO_HUB_SERVICE:-"$PAYLOAD_DIR/systemd/backuppo-hub.service"}

test -f "$BINARY" || { echo "backuppo-hub non trovato: $BINARY" >&2; exit 1; }
test -f "$CONFIG_TEMPLATE" || { echo "configurazione iniziale non trovata: $CONFIG_TEMPLATE" >&2; exit 1; }
test -f "$SERVICE_TEMPLATE" || { echo "unit systemd non trovata: $SERVICE_TEMPLATE" >&2; exit 1; }
command -v systemctl >/dev/null 2>&1 || { echo "systemd non disponibile" >&2; exit 1; }
command -v openssl >/dev/null 2>&1 || { echo "openssl è necessario per generare le chiavi" >&2; exit 1; }

if ! getent group backuppo-hub >/dev/null 2>&1; then groupadd --system backuppo-hub; fi
if ! id backuppo-hub >/dev/null 2>&1; then useradd --system --gid backuppo-hub --home-dir /var/lib/backuppo-hub --shell /usr/sbin/nologin backuppo-hub; fi

install -d -m 0750 -o backuppo-hub -g backuppo-hub /var/lib/backuppo-hub
install -d -m 0750 -o root -g backuppo-hub /etc/backuppo-hub
install -m 0755 "$BINARY" /usr/local/bin/backuppo-hub
if [ ! -f /etc/backuppo-hub/config.yaml ]; then install -m 0640 -o root -g backuppo-hub "$CONFIG_TEMPLATE" /etc/backuppo-hub/config.yaml; fi
if [ ! -f /etc/backuppo-hub/backuppo-hub.env ]; then
  JWT_SECRET=$(openssl rand -hex 32)
  POLICY_KEY=$(openssl rand -base64 32 | tr -d '\n')
  {
    echo "HUB_JWT_SECRET=$JWT_SECRET"
    echo "HUB_POLICY_SIGNING_KEY=$POLICY_KEY"
  } > /etc/backuppo-hub/backuppo-hub.env
  chown root:backuppo-hub /etc/backuppo-hub/backuppo-hub.env
  chmod 0640 /etc/backuppo-hub/backuppo-hub.env
fi
install -m 0644 "$SERVICE_TEMPLATE" /etc/systemd/system/backuppo-hub.service
/usr/local/bin/backuppo-hub check --config /etc/backuppo-hub/config.yaml

systemctl daemon-reload
systemctl enable backuppo-hub.service
systemctl restart backuppo-hub.service
echo "Backuppo Hub installato su http://127.0.0.1:8080"
echo "Crea il primo amministratore con: backuppo-hub create-user --config /etc/backuppo-hub/config.yaml --username admin --password-env HUB_ADMIN_PASSWORD"
