#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPOSITORY_DIR="$(cd -- "${SCRIPT_DIR}/.." && pwd)"
TSDR_LIBRARY_DIR="${REPOSITORY_DIR}/TempestSDR/bin/LINUX/X64"

make -C "${REPOSITORY_DIR}/TempestSDR" all
make -C "${REPOSITORY_DIR}/TSDRPlugin_Soapy" all

export LD_LIBRARY_PATH="${TSDR_LIBRARY_DIR}${LD_LIBRARY_PATH:+:${LD_LIBRARY_PATH}}"
exec cargo run \
    --manifest-path "${REPOSITORY_DIR}/rust/Cargo.toml" \
    --package tempest-ui \
    "$@"
