# Oikome Plugins

Official plugin packages for [Oikome](https://github.com/oikome/oikome).

This repository is independent of the main application's source and release
cycle. Packages in `packages/` use the same public `oikome-plugin.json` manifest
and runtime rules as external packages installed from a local folder. Official
status grants no additional permissions.

## Catalog and releases

`scripts/build.py` validates every package, creates a deterministic ZIP per
package version and writes `index.json`. Each index entry carries the complete
manifest, versioned download URL, byte size and SHA-256 digest. No generated ZIP
or index is committed to the source repository.

The catalog workflow publishes those files to the `catalog` GitHub Release.
Oikome reads `releases/latest/download/index.json`, downloads the selected
package, verifies its size and SHA-256, validates that its manifest matches the
index, and then uses the ordinary plugin installation lifecycle. HTTPS and the
release digest provide transport and content integrity; publisher signing is a
separate future trust feature.

The `note-count` package is intentionally not embedded in Oikome. It exercises
the external-official path: browse the official catalog, download, install, add
its Home card and explicitly choose what it may read.

## Package kinds

Dashboard cards use the existing browser sandbox. Source declarations configure
existing node connectors without loading package code. A preview that names an
`entry` runs that WebAssembly component in the node's sandbox, once per file: it
reads the one file it is given and nothing else, within the node's limits of
memory, time and result size; a preview without an entry is metadata only. MCP
client declarations use the generic manual/JSON/TOML setup flow. No capability
inherits another kind's runtime privileges.

## Packages with code

A package ships only what Oikome loads: its manifest, web files and built
components. The source of a component is in `sources/<id>/`, outside the
package, with the contract it is built against (`wit/preview.wit`, a copy of
the main repository's `crates/oikome-sandbox/wit/preview.wit`).

`sources/raw-preview` is RAW Preview: the JPEG a camera stored inside its RAW
file (CR2, CR3, NEF, ARW, DNG, ORF, PEF, RAF), taken out as it is. Nothing is
developed and no RAW decoder is used. `scripts/build-raw-preview.sh` runs its
tests, builds it for `wasm32-unknown-unknown` and writes
`packages/raw-preview/preview.wasm`, which is committed: the catalog workflow
packages what is in `packages/` and builds no code. Change the source and the
component together.

## Add an official package

1. Create `packages/<id>/oikome-plugin.json` and its web files, if any.
2. Follow the public SDK contract in the main repository's
   `packages/plugin-sdk/README.md` and provide every app language.
3. Run `python3 scripts/build.py` and inspect `dist/index.json`.
4. Review the package's permissions, fidelity limits and real-node behavior in
   the main repository before publishing it.

All packages and tooling are Apache-2.0.
