#!/bin/bash
# Bounce an ethernet WAN: disable, wait, re-enable.
# Usage: reset-ethernet.sh <interface>
set -euo pipefail

if [[ $# -lt 1 ]]; then
  echo "usage: $0 <ethernet-interface>" >&2
  exit 1
fi

IFACE="$1"
runcfg=/opt/vyatta/sbin/vyatta-cfg-cmd-wrapper

echo "Resetting ethernet interface ${IFACE}"
$runcfg begin
$runcfg set interfaces ethernet "$IFACE" disable
$runcfg commit
sleep 10
$runcfg delete interfaces ethernet "$IFACE" disable
$runcfg commit
$runcfg end
echo "Ethernet interface ${IFACE} reset"
