#!/bin/bash
# Disable an ethernet interface.
# Usage: disable-ethernet.sh <interface>
set -euo pipefail

if [[ $# -lt 1 ]]; then
  echo "usage: $0 <ethernet-interface>" >&2
  exit 1
fi

IFACE="$1"
runcfg=/opt/vyatta/sbin/vyatta-cfg-cmd-wrapper

$runcfg begin
$runcfg set interfaces ethernet "$IFACE" disable
$runcfg commit
$runcfg end
