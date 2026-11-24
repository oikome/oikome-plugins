#!/usr/bin/env python3
# Copyright 2025 The Oikome Authors
# SPDX-License-Identifier: Apache-2.0

import hashlib
import json
from pathlib import Path
import zipfile

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "dist"
DOWNLOAD = "https://github.com/oikome/oikome-plugins/releases/download/catalog"
MAX_FILES = 200
MAX_BYTES = 8 * 1024 * 1024


def read_manifest(package: Path) -> dict:
    manifest = json.loads((package / "oikome-plugin.json").read_text())
    assert manifest["id"] == package.name
    assert manifest["by"] == "Oikome"
    capabilities = manifest.get("capabilities", {})
    cards = capabilities.get("cards", [])
    for card in cards:
        assert (package / card["entry"]).is_file()
    # A preview's code is in the package: the built component it names.
    for preview in capabilities.get("preview", []):
        if "entry" in preview:
            assert (package / preview["entry"]).read_bytes().startswith(b"\0asm")
    assert cards or capabilities.get("sources") or capabilities.get("preview") or capabilities.get("mcpClients")
    return manifest


def build_package(package: Path) -> dict:
    manifest = read_manifest(package)
    files = sorted(path for path in package.rglob("*") if path.is_file())
    assert len(files) <= MAX_FILES
    assert sum(path.stat().st_size for path in files) <= MAX_BYTES
    name = f"{package.name}-{manifest['version']}.zip"
    target = OUT / name
    with zipfile.ZipFile(target, "w", zipfile.ZIP_DEFLATED) as archive:
        for path in files:
            relative = path.relative_to(package)
            assert not relative.is_absolute() and ".." not in relative.parts
            assert not any(part.startswith(".") for part in relative.parts)
            info = zipfile.ZipInfo(relative.as_posix(), (2025, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = 0o100644 << 16
            archive.writestr(info, path.read_bytes())
    data = target.read_bytes()
    return {
        "manifest": manifest,
        "download": f"{DOWNLOAD}/{name}",
        "sha256": hashlib.sha256(data).hexdigest(),
        "bytes": len(data),
    }


def main() -> None:
    OUT.mkdir(exist_ok=True)
    entries = [build_package(package) for package in sorted((ROOT / "packages").iterdir()) if package.is_dir()]
    index = {"schemaVersion": 1, "plugins": entries}
    (OUT / "index.json").write_text(json.dumps(index, ensure_ascii=False, indent=2) + "\n")
    print(f"Built {len(entries)} official plugin packages in {OUT}")


if __name__ == "__main__":
    main()
