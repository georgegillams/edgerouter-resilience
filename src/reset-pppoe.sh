#!/bin/bash
# Reset a PPPoE interface: disconnect, wait, reconnect.
# Usage: reset-pppoe.sh <interface>
set -euo pipefail

if [[ $# -lt 1 ]]; then
  echo "usage: $0 <pppoe-interface>" >&2
  exit 1
fi

IFACE="$1"
runop=/opt/vyatta/bin/vyatta-op-cmd-wrapper

echo "Resetting PPPoE interface ${IFACE}"
$runop disconnect interface "$IFACE"
sleep 10
$runop connect interface "$IFACE"
echo "PPPoE interface ${IFACE} reset"
