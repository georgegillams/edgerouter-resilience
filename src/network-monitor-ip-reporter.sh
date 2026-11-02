#!/bin/bash
DIR="$(cd "$(dirname "$0")" && pwd)"
exec "$DIR/network-monitor-ip-reporter" "$@"
