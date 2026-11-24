#!/bin/sh
# Copyright 2025 The Oikome Authors
# SPDX-License-Identifier: Apache-2.0
#
# Builds packages/raw-preview/preview.wasm from sources/raw-preview.
# Needs Rust with the wasm32-unknown-unknown target (rustup target add).
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
(cd "$root/sources/raw-preview" && cargo test --locked && cargo build --release --locked --target wasm32-unknown-unknown)
(cd "$root/sources/componentize" && cargo run --release --locked -- \
  "$root/sources/raw-preview/target/wasm32-unknown-unknown/release/oikome_raw_preview.wasm" \
  "$root/packages/raw-preview/preview.wasm")
ls -l "$root/packages/raw-preview/preview.wasm"
