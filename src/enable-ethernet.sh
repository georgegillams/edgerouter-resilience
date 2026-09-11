#!/bin/bash
# Enable an ethernet interface (clear disable).
# Usage: enable-ethernet.sh <interface>
set -euo pipefail

if [[ $# -lt 1 ]]; then
  echo "usage: $0 <ethernet-interface>" >&2
  exit 1
fi

IFACE="$1"
runcfg=/opt/vyatta/sbin/vyatta-cfg-cmd-wrapper

$runcfg begin
$runcfg delete interfaces ethernet "$IFACE" disable
$runcfg commit
$runcfg end
