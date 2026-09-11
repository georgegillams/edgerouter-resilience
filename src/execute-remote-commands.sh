#!/bin/bash
DIR="$(cd "$(dirname "$0")" && pwd)"
exec "$DIR/execute-remote-commands" "$@"
