#!/usr/bin/env python3
"""Collect exact locked registry notices, with explicitly labelled SPDX fallbacks.

Standard library only. No SBOM or linked-binary/compliance completeness claim.
Reads checksum-verified Cargo cache archives; never builds dependency code.

CLI (desktop bundle): python scripts/release_licenses.py --manifest-dir desktop/src-tauri --out <empty dir>
"""
from __future__ import annotations

import argparse
import hashlib
import io
import json
from pathlib import Path, PurePosixPath
import re
import subprocess
import tarfile
import tomllib
from urllib.request import Request, urlopen

SPDX_REVISION = "d46e94e2c78ceede1cfc63cfa0396472d2798d4c"  # SPDX license-list-data v3.27.0
SUPPORTED = {"MIT", "Apache-2.0", "Unicode-3.0", "Zlib", "Unlicense", "CC0-1.0",
             "BSD-3-Clause", "LGPL-2.1-or-later", "BSL-1.0",
             # Desktop (Tauri) tree, reviewed 2026-11-23.
             "0BSD", "MIT-0", "ISC", "MPL-2.0"}
# SPDX license exceptions, only valid after WITH; their standard text is shipped alongside.
EXCEPTIONS = {"LLVM-exception"}
NOTICE = re.compile(r"^(?:licen[cs]e|copying|copyright|notice|unlicense)(?:[._-].*)?$", re.I)
TERMS = re.compile(r"^(?:licen[cs]e|copying|unlicense)(?:[._-].*)?$", re.I)


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


WORKSPACE_SCOPE = ("All external packages in workspace Cargo.lock, including backend, host/build and "
                   "optional/target-specific dependencies; not proof of linkage in frontend WASM.")


def collect_licenses(root: Path, lock_bytes: bytes, scope: str = WORKSPACE_SCOPE) -> dict[str, bytes]:
    result = subprocess.run(["cargo", "metadata", "--locked", "--offline", "--format-version", "1"],
                            cwd=root, capture_output=True, text=True, encoding="utf-8")
    if result.returncode:
        raise ValueError("License collection needs cached locked Cargo packages; run cargo fetch --locked: " + result.stderr.strip())
    metadata = json.loads(result.stdout)
    packages = {(p["name"], p["version"], p["source"]): p for p in metadata["packages"] if p["source"]}
    locked = [p for p in tomllib.loads(lock_bytes.decode())["package"] if "source" in p]
    expected = {(p["name"], p["version"], p["source"]) for p in locked}
    if set(packages) != expected:
        raise ValueError("Cargo metadata must cover exactly the lockfile's external packages")
    entries: dict[str, bytes] = {}
    records = []
    standard_texts: dict[str, bytes] = {}
    for locked_package in sorted(locked, key=lambda p: (p["name"], p["version"], p["source"])):
        key = (locked_package["name"], locked_package["version"], locked_package["source"])
        package = packages[key]
        if not package["source"].startswith("registry+") or not locked_package.get("checksum"):
            raise ValueError(f"License collection requires reviewed support for non-registry package: {key}")
        identity = f"{package['name']}-{package['version']}"
        crate_root = Path(package["manifest_path"]).parent
        cache = crate_root.parent.parent.parent / "cache" / crate_root.parent.name / (identity + ".crate")
        crate_bytes = cache.read_bytes()
        if digest(crate_bytes) != locked_package["checksum"]:
            raise ValueError(f"Cargo cache archive checksum mismatch: {identity}")
        prefix = f"licenses/rust/{identity}/"
        notices = []
        copyright_evidence = []
        has_terms = False
        with tarfile.open(fileobj=io.BytesIO(crate_bytes), mode="r:gz") as archive:
            for member in archive.getmembers():
                if not member.isfile():
                    continue
                relative = PurePosixPath(member.name).relative_to(identity).as_posix()
                if ".." in PurePosixPath(relative).parts or "\\" in relative:
                    raise ValueError(f"Unsafe registry archive entry: {identity}/{relative}")
                data = archive.extractfile(member).read()
                if NOTICE.fullmatch(PurePosixPath(relative).name) or relative == "Cargo.toml.orig" or (relative == "Cargo.toml" and not package.get("license")):
                    path = prefix + relative
                    entries[path] = data
                    notices.append({"path": path, "sha256": digest(data), "origin": "published-crate", "source_path": relative})
                    has_terms |= bool(TERMS.fullmatch(PurePosixPath(relative).name))
                try:
                    lines = data.decode("utf-8").splitlines()
                except UnicodeDecodeError:
                    continue
                for line_number, line in enumerate(lines, 1):
                    if re.search(r"copyright|©", line, re.I):
                        copyright_evidence.append({"source_path": relative, "line": line_number, "text": line,
                                                   "source_sha256": digest(data)})
        declaration = package.get("license") or ""
        # Cargo's legacy slash syntax means OR; retain the original declaration too.
        normalized = declaration.replace("/", " OR ")
        atoms = set(re.findall(r"[A-Za-z0-9][A-Za-z0-9.+-]*", normalized)) - {"AND", "OR", "WITH"}
        known = SUPPORTED | EXCEPTIONS
        grammar = r"\s*(?:\(|\)|AND\b|OR\b|WITH\b|" + "|".join(re.escape(x) for x in sorted(known, key=len, reverse=True)) + r")\s*"
        exceptions_used = atoms & EXCEPTIONS
        exceptions_after_with = set(re.findall(r"\bWITH\s+([A-Za-z0-9.+-]+)", normalized))
        if (not atoms or not atoms <= known or re.sub(grammar, "", normalized)
                or not (atoms - EXCEPTIONS) or exceptions_after_with != exceptions_used):
            raise ValueError(f"Unsupported/unknown license declaration for {identity}: {declaration!r}")
        if not has_terms:
            for license_id in sorted(atoms):
                if license_id not in standard_texts:
                    url = f"https://raw.githubusercontent.com/spdx/license-list-data/{SPDX_REVISION}/text/{license_id}.txt"
                    with urlopen(Request(url, headers={"User-Agent": "flood-map-release-license-collector"}), timeout=30) as response:
                        data = response.read()
                    if not data.strip():
                        raise ValueError(f"Empty SPDX standard text: {license_id}")
                    standard_texts[license_id] = data
                path = f"licenses/spdx/{license_id}.txt"
                entries[path] = standard_texts[license_id]
                notices.append({"path": path, "sha256": digest(entries[path]), "origin": "spdx-standard-terms",
                                "url": f"https://raw.githubusercontent.com/spdx/license-list-data/{SPDX_REVISION}/text/{license_id}.txt"})
        evidence = {"authors_as_declared": package["authors"], "license_as_declared": declaration,
                    "repository_as_declared": package.get("repository"), "copyright_lines": copyright_evidence,
                    "warning": "Authors are manifest metadata, not an inferred copyright holder/year. Standard terms do not reconstruct missing upstream notices."}
        evidence_path = prefix + "ATTRIBUTION_EVIDENCE.json"
        entries[evidence_path] = (json.dumps(evidence, ensure_ascii=False, indent=2) + "\n").encode()
        notices.append({"path": evidence_path, "sha256": digest(entries[evidence_path]), "origin": "published-crate-metadata-and-copyright-lines"})
        records.append({"name": key[0], "version": key[1], "source": key[2], "crate_sha256": locked_package["checksum"],
                        "license": declaration, "authors": package["authors"], "repository": package.get("repository"),
                        "missing_upstream_notice": not has_terms, "standard_terms_used": not has_terms,
                        "files": notices})
    inventory = {"schema": 1, "scope": scope,
                 "limitations": "Not an SBOM, legal clearance, or complete upstream notice reconstruction. missing_upstream_notice/standard_terms_used require maintainer review before publication. NPM test/deploy tools, Rust sysroot and external map/API services are outside this Cargo inventory; MapLibre/font/data notices ship separately.",
                 "spdx_standard_text_revision": SPDX_REVISION, "cargo_lock_sha256": digest(lock_bytes), "packages": records}
    entries["DEPENDENCY_LICENSES.json"] = (json.dumps(inventory, ensure_ascii=False, indent=2) + "\n").encode()
    missing = [p for p in records if p["missing_upstream_notice"]]
    print(f"Dependency license evidence: {len(records)} locked registry packages; {len(missing)} missing upstream texts use labelled SPDX terms (review before publication)")
    return entries


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest-dir", required=True, help="Cargo workspace root with Cargo.lock")
    parser.add_argument("--out", required=True, help="New or empty output directory")
    args = parser.parse_args()
    root = Path(args.manifest_dir).resolve()
    out = Path(args.out).resolve()
    if out.exists() and any(out.iterdir()):
        raise SystemExit(f"Output directory must be empty: {out}")
    scope = (f"All external packages in {root.name}/Cargo.lock (desktop shell), including build and "
             "target-specific dependencies for other platforms; not proof of linkage in a given binary. "
             "Operating-system WebView (WebView2/WebKitGTK) and system libraries are not included.")
    entries = collect_licenses(root, (root / "Cargo.lock").read_bytes(), scope)
    for relative, data in entries.items():
        path = out / PurePosixPath(relative)
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
    print(f"Wrote {len(entries)} license evidence files to {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
