#!/usr/bin/env sh
set -eu

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_root=$(dirname -- "$script_dir")
jobs=${CARGO_BUILD_JOBS:-1}

cd "$repo_root"
make slate-bin CARGO_BUILD_JOBS="$jobs"
exec "$repo_root/slate" "$@"
