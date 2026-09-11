#!/usr/bin/env bash
# Cross-compile static 32-bit MIPS binaries for EdgeRouter into dist/.
#
# EdgeOS userspace is ELF 32-bit MSB (o32). 64-bit n64 binaries fail with
# "Accessing a corrupted shared library".
set -euo pipefail

ROOT="$(cd "$(dirname "$0")" && pwd)"
cd "$ROOT"

TARGET="${TARGET:-mips-unknown-linux-musl}"
IMAGE_NAME="${IMAGE_NAME:-edgerouter-scripts-er6p}"
DIST="$ROOT/dist"
HOST_TARGET_DIR="$ROOT/target/er6p"
CONTAINER_TARGET_DIR="/work/target/er6p"

if [[ "${TARGET}" != "mips-unknown-linux-musl" ]]; then
  echo "error: this build.sh only supports mips-unknown-linux-musl (got ${TARGET})" >&2
  exit 1
fi

if ! command -v docker >/dev/null 2>&1; then
  echo "error: Docker not found. Install and start Docker Desktop." >&2
  exit 1
fi

if ! docker info >/dev/null 2>&1; then
  echo "error: Docker is not reachable. Start Docker Desktop and retry." >&2
  exit 1
fi

echo "Building Docker image ${IMAGE_NAME}..."
echo "(pulls 32-bit mips musl toolchain + Ubuntu 24.04; may take a few minutes the first time)"
# Platform flag on `docker build` (not FROM) keeps Dockerfile portable on Apple Silicon.
docker build \
  --platform linux/amd64 \
  -f "${ROOT}/docker/Dockerfile.er6p" \
  -t "${IMAGE_NAME}" \
  "${ROOT}/docker"

mkdir -p "${HOST_TARGET_DIR}"

echo "Compiling release binaries for ${TARGET}..."
docker run --rm --platform linux/amd64 \
  -v "${ROOT}:/work" \
  -v "${HOME}/.cargo/registry:/root/.cargo/registry" \
  -v "${HOME}/.cargo/git:/root/.cargo/git" \
  -e CARGO_TARGET_DIR="${CONTAINER_TARGET_DIR}" \
  -e CARGO_HOME=/root/.cargo \
  -w /work \
  "${IMAGE_NAME}" \
  cargo build --release --target "${TARGET}" \
    -Z build-std=std,panic_abort

BIN_DIR="${HOST_TARGET_DIR}/${TARGET}/release"
for bin in auto-reset-pppoe upload-reset-logs execute-remote-commands; do
  if [[ ! -f "${BIN_DIR}/${bin}" ]]; then
    echo "error: missing binary: ${BIN_DIR}/${bin}" >&2
    exit 1
  fi
done

if command -v file >/dev/null 2>&1; then
  for bin in auto-reset-pppoe upload-reset-logs execute-remote-commands; do
    info="$(file "${BIN_DIR}/${bin}")"
    echo "${info}"
    if echo "${info}" | grep -qi 'dynamically linked'; then
      echo "error: ${bin} is dynamically linked — will fail on EdgeOS" >&2
      exit 1
    fi
    if echo "${info}" | grep -qi '64-bit'; then
      echo "error: ${bin} is 64-bit — EdgeOS userspace is 32-bit MIPS" >&2
      exit 1
    fi
    if ! echo "${info}" | grep -qi '32-bit'; then
      echo "error: ${bin} is not 32-bit MIPS" >&2
      exit 1
    fi
  done
fi

echo "Assembling ${DIST}..."
rm -rf "${DIST}"
mkdir -p "${DIST}"

cp "${BIN_DIR}/auto-reset-pppoe" "${DIST}/"
cp "${BIN_DIR}/upload-reset-logs" "${DIST}/"
cp "${BIN_DIR}/execute-remote-commands" "${DIST}/"
cp "${ROOT}"/src/*.sh "${DIST}/"
cp "${ROOT}/config.example.toml" "${DIST}/"

if [[ -f "${ROOT}/config.toml" ]]; then
  cp "${ROOT}/config.toml" "${DIST}/"
  echo "Included local config.toml"
else
  echo "Note: no config.toml — copy config.example.toml on the router and edit it."
fi

chmod +x "${DIST}"/* "${DIST}"/*.sh 2>/dev/null || true
chmod a-x "${DIST}"/*.toml 2>/dev/null || true

echo "Done. Contents of dist/:"
ls -la "${DIST}"
echo
echo "Deploy tip: scp to /tmp then sudo cp into /config/scripts/"
echo "On router: file /config/scripts/auto-reset-pppoe"
echo "  expect: ELF 32-bit MSB executable, MIPS, MIPS32 rel2 … statically linked"
