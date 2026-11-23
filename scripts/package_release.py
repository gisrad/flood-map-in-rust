#!/usr/bin/env python3
"""Validate and package a tagged source tree and an already-tested Trunk build.

Python 3.11+, standard library only. Paths are relative to the repository root,
not the caller's current directory. --check-only never writes archives. With
--worktree it includes non-ignored uncommitted files and needs no existing tag.
Full packaging reads source/metadata from Git, never local configuration.
"""

from __future__ import annotations

import argparse
import gzip
import hashlib
from html.parser import HTMLParser
import io
import json
import os
from pathlib import Path, PurePosixPath
import re
import sys
import subprocess
import tarfile
import time
from urllib.parse import unquote, urlsplit
import zipfile

from release_licenses import TERMS, collect_licenses

if sys.version_info < (3, 11):
    raise SystemExit("Python 3.11 or newer is required")
import tomllib

ROOT = Path(__file__).resolve().parent.parent
PROJECT = "flood-map-in-rust"
SEMVER = r"(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)"
DISCLAIMER = "Indikasi model, bukan peringatan resmi"
DOCS = (
    "LICENSE", "DATA_LICENSES.md", "THIRD_PARTY_NOTICES.md", "CITATION.cff",
    "CHANGELOG.md", "docs/API.md", "docs/METODOLOGI.md", "docs/RELEASING.md",
    "crates/flood-web/vendor/LICENSE.txt", "crates/flood-web/assets/fonts/OFL.txt",
)
REQUIRED = DOCS + (
    "README.md", "Cargo.toml", "Cargo.lock", "package.json", "package-lock.json",
    ".github/workflows/ci.yml", ".github/workflows/release.yml",
    ".github/workflows/deploy.yml", "scripts/security-check.ps1",
    "tests/e2e/playwright.config.mjs", "tests/e2e/support/test.mjs",
    "tests/e2e/fixtures/server.mjs", "tests/e2e/fixtures/snapshot.mjs",
    "crates/flood-web/Trunk.toml", "crates/flood-web/index.html",
    "crates/flood-worker/wrangler.toml.example",
    "scripts/package_release.py", "scripts/check_wasm_size.mjs", "scripts/release_licenses.py",
    "desktop/src-tauri/Cargo.toml", "desktop/src-tauri/Cargo.lock", "desktop/src-tauri/tauri.conf.json",
    "desktop/src-tauri/src/main.rs", "desktop/launcher/index.html", ".github/workflows/desktop.yml",
)
GENERATED_DIRS = {
    "target", "dist", "build", "node_modules", ".git", ".wrangler", "__pycache__",
    "test-results", "playwright-report", "blob-report", "results", "report",
}

def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def build_provenance(source: Source, dist: Path) -> None:
    """Record the builder, not the later packaging machine; no host paths/secrets."""
    commit = git("rev-parse", "HEAD").decode().strip()
    if git("diff", "--name-only", "HEAD", "--"):
        raise ValueError("Commit tracked changes before building release assets")
    files = validate_dist(source, dist, require_provenance=False)
    tools = {}
    for name, command in (
        ("rustc", ["rustc", "-Vv"]), ("cargo", ["cargo", "-V"]),
        ("trunk", ["trunk", "--version"]), ("node", ["node", "--version"]),
    ):
        result = subprocess.run(command, capture_output=True, text=True, check=True)
        tools[name] = result.stdout.strip()
    record = {
        "schema": 1, "source_commit": commit,
        "cargo_lock_sha256": sha256(source.read("Cargo.lock")),
        "target": "wasm32-unknown-unknown",
        "command": "trunk build --release --locked",
        "tools": tools,
        "backend": "not included; requires a separately built compatible same-origin /api/* Worker",
        "assets": {p: sha256(f.read_bytes()) for p, f in sorted(files.items()) if p != "BUILD_PROVENANCE.json"},
    }
    (dist / "BUILD_PROVENANCE.json").write_text(json.dumps(record, indent=2) + "\n", encoding="utf-8")



def git(*args: str) -> bytes:
    result = subprocess.run(
        ["git", "-C", str(ROOT), *args], capture_output=True,
        env={**os.environ, "GIT_OPTIONAL_LOCKS": "0"},
    )
    if result.returncode:
        raise ValueError(result.stderr.decode("utf-8", "replace").strip())
    return result.stdout


class Source:
    def __init__(self, revision: str | None):
        self.commit = git("rev-parse", "--verify", f"{revision}^{{commit}}").decode().strip() if revision else None
        if self.commit:
            rows = git("ls-tree", "-rz", "--full-tree", self.commit).split(b"\0")
            self.paths = set()
            for row in filter(None, rows):
                mode_type, raw_path = row.split(b"\t", 1)
                mode, kind, _oid = mode_type.split()
                path = raw_path.decode("utf-8")
                if kind != b"blob" or mode == b"120000":
                    raise ValueError(f"Source cannot contain symlink/submodule: {path}")
                self.paths.add(path)
        else:
            self.paths = set(filter(None, git("ls-files", "--cached", "--others", "--exclude-standard", "-z").decode().split("\0")))

    def read(self, path: str) -> bytes:
        if path not in self.paths:
            raise ValueError(f"Required source file is missing: {path}")
        if self.commit:
            return git("show", f"{self.commit}:{path}")
        local = ROOT / path
        if local.is_symlink() or not local.is_file():
            raise ValueError(f"Required regular source file is missing: {path}")
        return local.read_bytes()

    def text(self, path: str) -> str:
        return self.read(path).decode("utf-8-sig")


def forbidden(path: str, web: bool = False) -> bool:
    parts = PurePosixPath(path).parts
    name = parts[-1].lower()
    if any(part.lower() in GENERATED_DIRS for part in parts):
        return True
    if name == "wrangler.toml" or name.startswith((".env", ".dev.vars")):
        return True
    if name == ".trunk-lock" or name.endswith((".log", ".pyc")):
        return True
    if path.startswith("docs/qa-"):
        return True
    return web and (
        any(p.lower() in {"tests", "test", "fixtures", "fixture", "scripts", ".github"} for p in parts)
        or any(p.startswith(".") and p != ".well-known" for p in parts)
    )


def validate_source(source: Source, tag: str | None) -> tuple[str, str]:
    for path in REQUIRED:
        if not source.read(path).strip():
            raise ValueError(f"Required file is empty: {path}")
    bad = sorted(p for p in source.paths if forbidden(p))
    if bad:
        raise ValueError("Local/generated files must not enter source archive: " + ", ".join(bad))
    if not any(p.startswith("tests/e2e/") and p.endswith((".js", ".mjs", ".ts")) for p in source.paths):
        raise ValueError("Tracked tests/e2e test code is required")
    workspace = tomllib.loads(source.text("Cargo.toml"))["workspace"]
    version = workspace["package"]["version"]
    if not re.fullmatch(SEMVER, version):
        raise ValueError(f"Workspace version must be MAJOR.MINOR.PATCH: {version}")
    if tag and tag != f"v{version}":
        raise ValueError(f"Tag {tag} does not match workspace version {version}")
    lock = tomllib.loads(source.text("Cargo.lock"))
    for member in workspace["members"]:
        package = tomllib.loads(source.text(f"{member}/Cargo.toml"))["package"]
        if package.get("version") != {"workspace": True}:
            raise ValueError(f"{member} must inherit version.workspace = true")
        matches = [p for p in lock["package"] if p["name"] == package["name"] and "source" not in p]
        if len(matches) != 1 or matches[0]["version"] != version:
            raise ValueError(f"Cargo.lock version mismatch for {package['name']}")
    cff = re.findall(r"^version:\s*['\"]?([^\s'\"]+)['\"]?\s*$", source.text("CITATION.cff"), re.M)
    if cff != [version]:
        raise ValueError(f"CITATION.cff version must be {version}")
    desktop_crate = tomllib.loads(source.text("desktop/src-tauri/Cargo.toml"))["package"]
    desktop_conf = json.loads(source.text("desktop/src-tauri/tauri.conf.json"))
    if desktop_crate.get("version") != version or desktop_conf.get("version") != version:
        raise ValueError(f"Desktop Cargo.toml and tauri.conf.json versions must be {version}")
    if desktop_conf.get("identifier") != "id.transbulent.hidrosentinel":
        raise ValueError("Desktop identifier must stay id.transbulent.hidrosentinel (installed-app identity)")
    desktop_lock = tomllib.loads(source.text("desktop/src-tauri/Cargo.lock"))
    shell = [p for p in desktop_lock["package"] if p["name"] == desktop_crate["name"] and "source" not in p]
    if len(shell) != 1 or shell[0]["version"] != version:
        raise ValueError("desktop/src-tauri/Cargo.lock version mismatch")
    if json.loads(source.text("package.json")).get("version") != version:
        raise ValueError(f"package.json version must be {version}")
    changelog = source.text("CHANGELOG.md")
    heading = re.compile(rf"^##\s+\[?{re.escape(version)}\]?(?:\s.*)?$", re.M)
    matches = list(heading.finditer(changelog))
    if len(matches) != 1:
        raise ValueError(f"CHANGELOG.md must have exactly one ## [{version}] entry")
    tail = changelog[matches[0].end():]
    body = re.split(r"^#{1,2}\s", tail, maxsplit=1, flags=re.M)[0].strip()
    if not body or re.search(r"\b(?:TODO|TBD|FIXME)\b", body):
        raise ValueError("Release changelog must be substantive and contain no placeholder markers")
    if DISCLAIMER not in source.text("docs/RELEASING.md"):
        raise ValueError("docs/RELEASING.md must retain the model-indication notice")
    manifest = json.loads(source.text("package.json"))
    npm_lock = json.loads(source.text("package-lock.json"))
    if not manifest.get("scripts", {}).get("test:e2e"):
        raise ValueError("package.json must define test:e2e")
    pin = manifest.get("devDependencies", {}).get("@playwright/test", "")
    if not re.fullmatch(SEMVER, pin):
        raise ValueError("@playwright/test must have an exact version in package.json")
    if npm_lock.get("packages", {}).get("node_modules/@playwright/test", {}).get("version") != pin:
        raise ValueError("package-lock.json must pin the same @playwright/test version")
    print(f"Source version/metadata consistent: {version} ({source.commit or 'working tree'})")
    return version, body


class Links(HTMLParser):
    def __init__(self, html: str):
        super().__init__()
        self.refs: list[str] = []
        self.copies: list[tuple[str, str]] = []
        self.feed(html)

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        attrs = dict(attrs)
        if tag == "link" and attrs.get("rel") == "copy-dir" and "data-trunk" in attrs:
            href = attrs.get("href", "")
            self.copies.append((href, attrs.get("data-target-path", PurePosixPath(href).name)))
        if tag in {"link", "script", "img", "source"}:
            ref = attrs.get("src") or attrs.get("href")
            if ref:
                self.refs.append(ref)


def validate_dist(source: Source, dist: Path, require_provenance: bool = True) -> dict[str, Path]:
    if not dist.is_dir() or dist.is_symlink():
        raise ValueError(f"Missing regular dist directory: {dist}")
    files = {}
    for folder, dirs, names in os.walk(dist, followlinks=False):
        for name in list(dirs):
            path = Path(folder) / name
            if path.is_symlink():
                raise ValueError(f"Dist must not contain symlinks: {path}")
            if folder == str(dist) and name == ".stage":
                dirs.remove(name)
        for name in names:
            path = Path(folder) / name
            rel = path.relative_to(dist).as_posix()
            if rel == ".trunk-lock":
                continue  # Trunk's persistent lock is never a distributable asset.
            if path.is_symlink() or not path.is_file() or forbidden(rel, web=True):
                raise ValueError(f"Forbidden/non-regular dist entry: {rel}")
            files[rel] = path
    required = ["index.html", "vendor/LICENSE.txt", "vendor/maplibre-gl.js", "vendor/maplibre-gl.css", "assets/fonts/OFL.txt"]
    outputs = []
    for pattern in (r"flood-web-[0-9a-f]+_bg\.wasm", r"flood-web-[0-9a-f]+\.js", r"main-[0-9a-f]+\.css"):
        matches = [p for p in files if re.fullmatch(pattern, p)]
        if len(matches) != 1:
            raise ValueError(f"Expected exactly one Trunk output matching {pattern}; found {matches}")
        outputs.extend(matches)
    required.extend(outputs)
    for path in required:
        if path not in files or files[path].stat().st_size == 0:
            raise ValueError(f"Required dist file missing/empty: {path}")
    with files[outputs[0]].open("rb") as wasm:
        if wasm.read(8) != b"\x00asm\x01\x00\x00\x00":
            raise ValueError("Dist WebAssembly module has an invalid header")
    html = files["index.html"].read_text(encoding="utf-8")
    if "data-trunk" in html:
        raise ValueError("Dist index.html still has unprocessed Trunk directives")
    refs = Links(html).refs + re.findall(r"['\"](/[^'\"]+\.(?:js|wasm))['\"]", html)
    for ref in refs:
        url = urlsplit(ref)
        if url.scheme or url.netloc or not url.path or url.path == "/":
            continue
        path = unquote(url.path).lstrip("/")
        if path not in files:
            raise ValueError(f"Dist HTML references missing asset: {path}")
    copies = Links(source.text("crates/flood-web/index.html")).copies
    if not copies:
        raise ValueError("No Trunk copy-dir directives found in source index.html")
    copied = 0
    for directory, target in copies:
        prefix = f"crates/flood-web/{directory.rstrip('/')}/"
        for path in sorted(source.paths):
            if path.startswith(prefix):
                output = (PurePosixPath(target) / path[len(prefix):]).as_posix()
                if output not in files:
                    raise ValueError(f"Tracked static asset missing from dist: {output}")
                if files[output].read_bytes() != source.read(path):
                    raise ValueError(f"Dist static asset differs from selected source revision: {output}")
                copied += 1
    if require_provenance:
        if "BUILD_PROVENANCE.json" not in files:
            raise ValueError("Dist lacks BUILD_PROVENANCE.json; record it on the build machine with --record-build")
        record = json.loads(files["BUILD_PROVENANCE.json"].read_text(encoding="utf-8"))
        expected_commit = source.commit or git("rev-parse", "HEAD").decode().strip()
        if record.get("schema") != 1 or record.get("source_commit") != expected_commit:
            raise ValueError("Build provenance does not match selected source commit")
        if record.get("cargo_lock_sha256") != sha256(source.read("Cargo.lock")):
            raise ValueError("Build provenance Cargo.lock checksum mismatch")
        expected = {p: sha256(f.read_bytes()) for p, f in sorted(files.items()) if p != "BUILD_PROVENANCE.json"}
        if record.get("assets") != expected or not all(record.get("tools", {}).get(t) for t in ("rustc", "cargo", "trunk", "node")):
            raise ValueError("Build provenance is missing tool versions or assets have changed since recording")
    print(f"Dist complete: {len(files)} files, {copied} tracked static assets; Trunk lock excluded")
    return files


def release_notice(tag: str, commit: str) -> str:
    return f"""Hidrosentinel Nusantara - built web package
Tag: {tag}
Commit: {commit}

{DISCLAIMER}.
Ikuti informasi resmi BPBD/BNPB, BMKG dan pengelola sungai setempat.
Validasi Q2 masih pendahuluan; paket ini bukan sistem peringatan operasional.

site/ adalah web root statis hasil Trunk. Backend Worker TIDAK dibundel.
Frontend meminta /api/* pada origin yang sama dan memerlukan backend kompatibel;
HTML/WASM saja tidak menyediakan data API atau layanan notifikasi. Gunakan arsip
sumber untuk implementasi referensi crates/flood-worker dan konfigurasi sendiri.
Paket ini bukan installer/PWA/aplikasi luring. Layanan peta luar tetap diperlukan.

The API backend, credentials, live snapshots and synthetic test fixtures are not
bundled. Serve site/ over HTTP(S), not file://. See docs/API.md, docs/METODOLOGI.md
and docs/RELEASING.md for limits and setup; no production deployment is performed
by downloading or unpacking these files.

Kode MIT tidak melisensikan ulang data atau pustaka pihak ketiga. Pertahankan
LICENSE, DATA_LICENSES.md, THIRD_PARTY_NOTICES.md, CITATION.cff, lisensi MapLibre
dan OFL font; pertahankan juga licenses/, DEPENDENCY_LICENSES.json,
BUILD_PROVENANCE.json dan RELEASE_CONTENTS.json. Statistik turunan GSIM tetap CC BY-SA 3.0.
Inventaris Cargo mencakup seluruh workspace, bukan bukti linkage WASM. Entri
missing_upstream_notice memakai teks standar SPDX berlabel, bukan rekonstruksi
copyright upstream atau persetujuan hukum; wajib ditinjau sebelum publikasi.
SHA256SUMS memeriksa integritas unduhan, bukan autentikasi penerbit. Tidak ada
klaim tanda tangan, attestation penerbit, atau SBOM lengkap.
"""


def content_manifest(entries: dict[str, bytes], tag: str, commit: str, kind: str) -> bytes:
    return (json.dumps({
        "schema": 1, "tag": tag, "source_commit": commit, "kind": kind,
        "files": {p: sha256(data) for p, data in sorted(entries.items())},
    }, indent=2) + "\n").encode()


def verify_payload(entries: dict[str, bytes], tag: str, kind: str) -> str:
    """Validate consumer files from archive bytes, independent of the checkout."""
    for path in entries:
        parts = PurePosixPath(path).parts
        if not parts or path.startswith("/") or "\\" in path or ".." in parts:
            raise ValueError(f"Unsafe archive path: {path}")
    manifest = json.loads(entries["RELEASE_CONTENTS.json"])
    if manifest.get("schema") != 1 or manifest.get("tag") != tag or manifest.get("kind") != kind:
        raise ValueError("Release content manifest tag/schema/kind mismatch")
    actual = {p: sha256(data) for p, data in sorted(entries.items()) if p != "RELEASE_CONTENTS.json"}
    if manifest.get("files") != actual:
        raise ValueError("Archive content inventory is missing, extra or modified")
    required = set(DOCS) | {"BUILD_PROVENANCE.json", "DEPENDENCY_LICENSES.json", "Cargo.lock"}
    if kind == "source":
        required.update(REQUIRED)
        workspace = tomllib.loads(entries["Cargo.toml"].decode())["workspace"]
        for member in workspace["members"]:
            required.add(f"{member}/Cargo.toml")
            if not any(entries.get(f"{member}/src/{entry}", b"").strip() for entry in ("lib.rs", "main.rs")):
                raise ValueError(f"Source archive has no crate entrypoint: {member}")
        if not any(p.startswith("tests/e2e/") and p.endswith((".js", ".mjs", ".ts")) for p in entries):
            raise ValueError("Source archive has no runnable browser tests")
    else:
        required.update(("RELEASE_NOTICE.txt", "site/index.html", "site/vendor/maplibre-gl.js",
                         "site/vendor/maplibre-gl.css", "site/vendor/LICENSE.txt", "site/assets/fonts/OFL.txt"))
        for pattern in (r"site/flood-web-[0-9a-f]+_bg\.wasm", r"site/flood-web-[0-9a-f]+\.js", r"site/main-[0-9a-f]+\.css"):
            matches = [p for p in entries if re.fullmatch(pattern, p)]
            if len(matches) != 1:
                raise ValueError(f"Web archive lacks unambiguous runtime output: {pattern}")
            required.update(matches)
        for ref in Links(entries["site/index.html"].decode("utf-8")).refs:
            url = urlsplit(ref)
            if not url.scheme and not url.netloc and url.path and url.path != "/":
                required.add("site/" + unquote(url.path).lstrip("/"))
        provenance = json.loads(entries["BUILD_PROVENANCE.json"])
        actual_site = {p[5:]: sha256(data) for p, data in entries.items()
                       if p.startswith("site/") and p != "site/BUILD_PROVENANCE.json"}
        if provenance.get("assets") != actual_site:
            raise ValueError("Web archive differs from build-machine asset inventory")
        if entries.get("site/BUILD_PROVENANCE.json") != entries["BUILD_PROVENANCE.json"]:
            raise ValueError("Root and site build provenance differ")
    missing = sorted(p for p in required if not entries.get(p, b"").strip())
    if missing:
        raise ValueError("Archive lacks consumer-required files: " + ", ".join(missing))
    provenance = json.loads(entries["BUILD_PROVENANCE.json"])
    commit = manifest["source_commit"]
    if not re.fullmatch(r"[0-9a-f]{40}", commit) or provenance.get("source_commit") != commit:
        raise ValueError("Build and archive source commits differ")
    if provenance.get("cargo_lock_sha256") != sha256(entries["Cargo.lock"]):
        raise ValueError("Archive lockfile differs from build provenance")
    if provenance.get("schema") != 1 or provenance.get("target") != "wasm32-unknown-unknown" or not all(
        provenance.get("tools", {}).get(tool) for tool in ("rustc", "cargo", "trunk", "node")
    ):
        raise ValueError("Build provenance lacks required builder/target details")
    licenses = json.loads(entries["DEPENDENCY_LICENSES.json"])
    if licenses.get("schema") != 1 or licenses.get("cargo_lock_sha256") != sha256(entries["Cargo.lock"]):
        raise ValueError("Dependency license inventory schema/lockfile mismatch")
    lock = tomllib.loads(entries["Cargo.lock"].decode())
    expected = {(p["name"], p["version"], p["source"]) for p in lock["package"] if "source" in p}
    found = {(p["name"], p["version"], p["source"]) for p in licenses["packages"]}
    if expected != found or len(found) != len(licenses["packages"]):
        raise ValueError("Dependency license inventory does not cover locked external packages exactly")
    for package in licenses["packages"]:
        if not package.get("files"):
            raise ValueError(f"No license texts for {package['name']} {package['version']}")
        if not any(n["origin"] == "spdx-standard-terms" or
                   (n["origin"] == "published-crate" and TERMS.fullmatch(PurePosixPath(n["path"]).name))
                   for n in package["files"]):
            raise ValueError(f"Dependency has attribution metadata but no license terms: {package['name']}")
        used_standard = any(n["origin"] == "spdx-standard-terms" for n in package["files"])
        if package.get("standard_terms_used") != used_standard or package.get("missing_upstream_notice") != used_standard:
            raise ValueError(f"Dependency fallback flags hide/misstate missing upstream notice: {package['name']}")
        for notice in package["files"]:
            if sha256(entries[notice["path"]]) != notice["sha256"]:
                raise ValueError(f"Missing/modified dependency notice: {notice['path']}")
    print(f"Verified {kind} archive: {len(entries)} files, {len(found)} dependency license records, commit {commit}")
    return commit


def verify_assets(directory: Path, tag: str | None) -> None:
    sums = {}
    for line in (directory / "SHA256SUMS").read_text(encoding="utf-8").splitlines():
        match = re.fullmatch(r"([0-9a-f]{64})  (flood-map-in-rust-v" + SEMVER + r"-(?:web\.zip|source\.tar\.gz))", line)
        if not match or match[2] in sums:
            raise ValueError("Malformed or duplicate SHA256SUMS entry")
        sums[match[2]] = match[1]
    if tag is None and sums:
        tag = next(iter(sums)).removeprefix(PROJECT + "-").split("-")[0]
    expected = {f"{PROJECT}-{tag}-web.zip", f"{PROJECT}-{tag}-source.tar.gz"}
    if set(sums) != expected:
        raise ValueError("SHA256SUMS must list exactly the source and web archives for one tag")
    commits = []
    for name in sorted(expected):
        path = directory / name
        with path.open("rb") as stream:
            if hashlib.file_digest(stream, "sha256").hexdigest() != sums[name]:
                raise ValueError(f"Download checksum mismatch: {name}")
        kind = "web" if name.endswith("-web.zip") else "source"
        prefix = f"{PROJECT}-{tag}" + ("-web/" if kind == "web" else "/")
        if kind == "web":
            with zipfile.ZipFile(path) as archive:
                names = archive.namelist()
                if len(names) != len(set(names)) or any(
                    not n.startswith(prefix) or "\\" in n or ".." in PurePosixPath(n).parts
                    for n in names
                ):
                    raise ValueError("Duplicate/unsafe/unexpected ZIP path")
                if any(((m.external_attr >> 16) & 0o170000) not in (0, 0o100000, 0o040000) for m in archive.infolist()):
                    raise ValueError("Web archive contains links/special files")
                entries = {n[len(prefix):]: archive.read(n) for n in names if not n.endswith("/")}
        else:
            with tarfile.open(path, "r:gz") as archive:
                members = archive.getmembers()
                names = [m.name for m in members]
                if len(names) != len(set(names)) or any(
                    not (n + "/").startswith(prefix) or "\\" in n or ".." in PurePosixPath(n).parts
                    for n in names
                ):
                    raise ValueError("Duplicate/unsafe/unexpected tar path")
                if any(not (m.isfile() or m.isdir()) for m in members):
                    raise ValueError("Source archive contains links/special files")
                entries = {m.name[len(prefix):]: archive.extractfile(m).read() for m in members if m.isfile()}
        commits.append(verify_payload(entries, tag, kind))
    if len(set(commits)) != 1:
        raise ValueError("Source and web packages refer to different commits")


def release_notes(tag: str, commit: str, body: str, web_name: str, source_name: str) -> str:
    # release.yml attaches the desktop installers under these exact names.
    version = tag.removeprefix("v")
    exe = f"Hidrosentinel-Nusantara_{version}_x64-setup.exe"
    deb = f"hidrosentinel-nusantara_{version}_amd64.deb"
    return (
        f"> **{DISCLAIMER}.** Ikuti informasi resmi BPBD/BNPB, BMKG dan pengelola sungai.\n\n"
        f"Tag: `{tag}`\n\nCommit: `{commit}`\n\n{body}\n\n"
        f"## Unduhan\n\n- `{web_name}`: frontend statis (`site/`), lisensi dan dokumen. **Backend /api/* tidak dibundel**; perlu backend kompatibel pada origin yang sama.\n"
        f"- `{source_name}`: sumber Git, lockfile, workflow dan kode tes; bukan installer.\n"
        "- `SHA256SUMS`: jalankan `sha256sum -c SHA256SUMS` dari direktori unduhan. Checksum bukan autentikasi penerbit; belum ada tanda tangan/attestation atau SBOM lengkap.\n"
        f"- `{exe}`: installer Windows 10/11 per pengguna, **tanpa tanda tangan kode**.\n"
        f"- `{deb}`: paket Debian 12+/Ubuntu 22.04+; pasang dengan `sudo apt install ./{deb}`.\n"
        "- `SHA256SUMS-desktop`: jalankan `sha256sum -c SHA256SUMS-desktop` untuk kedua installer.\n\n"
        "Lisensi kode MIT tidak mengganti lisensi data/pustaka. Baca DATA_LICENSES.md dan THIRD_PARTY_NOTICES.md; GSIM tetap CC BY-SA 3.0. DEPENDENCY_LICENSES.json menandai teks upstream yang hilang dan fallback standar SPDX: tinjau sebelum publikasi. Inventaris workspace bukan SBOM atau bukti linkage WASM. BUILD_PROVENANCE.json adalah catatan builder, bukan attestation bertanda tangan. Rilis ini tidak melakukan deploy produksi.\n"
    )


def package(source: Source, tag: str, body: str, files: dict[str, Path], out: Path, notes: Path) -> None:
    commit = source.commit
    if git("rev-parse", "HEAD").decode().strip() != commit:
        raise ValueError("Checkout HEAD must equal the packaged revision; rebuild dist from that checkout")
    if git("diff", "--name-only", commit, "--"):
        raise ValueError("Tracked working-tree changes present; commit changes and rebuild before packaging")
    if out == notes.parent or out in notes.parents:
        raise ValueError("Release notes must be outside the download-assets directory")
    if notes.exists():
        raise ValueError(f"Existing release notes are not overwritten: {notes}")
    out.mkdir(parents=True, exist_ok=True)
    web_name = f"{PROJECT}-{tag}-web.zip"
    source_name = f"{PROJECT}-{tag}-source.tar.gz"
    if any(out.iterdir()):
        raise ValueError(f"Output directory must be empty; existing release assets are never overwritten: {out}")
    epoch = int(git("show", "-s", "--format=%ct", commit).strip())
    zip_time = time.gmtime(max(315532800, min(epoch, 4354819198)))[:6]
    support = collect_licenses(ROOT, source.read("Cargo.lock"))
    support["Cargo.lock"] = source.read("Cargo.lock")
    support["BUILD_PROVENANCE.json"] = files["BUILD_PROVENANCE.json"].read_bytes()
    root = f"{PROJECT}-{tag}-web"
    payload = {p: source.read(p) for p in DOCS}
    payload.update(support)
    payload["RELEASE_NOTICE.txt"] = release_notice(tag, commit).encode("utf-8")
    payload.update({f"site/{p}": f.read_bytes() for p, f in files.items()})
    payload["RELEASE_CONTENTS.json"] = content_manifest(payload, tag, commit, "web")
    verify_payload(payload, tag, "web")
    entries = {f"{root}/{p}": data for p, data in payload.items()}
    with zipfile.ZipFile(out / web_name, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for path, data in sorted(entries.items()):
            info = zipfile.ZipInfo(path, zip_time)
            info.create_system = 3
            info.external_attr = 0o100644 << 16
            info.compress_type = zipfile.ZIP_DEFLATED
            archive.writestr(info, data, compresslevel=9)
    # Git supplies all source bytes/modes; generated license evidence is additive. Disable
    # checkout EOL conversion (Windows core.autocrlf) so archive bytes equal committed blobs.
    tar = git("-c", "core.autocrlf=false", "-c", "core.eol=lf", "archive", "--format=tar", f"--prefix={PROJECT}-{tag}/", commit)
    source_root = f"{PROJECT}-{tag}/"
    with tarfile.open(fileobj=io.BytesIO(tar)) as archive:
        members = archive.getmembers()
        source_payload = {m.name[len(source_root):]: archive.extractfile(m).read() for m in members if m.isfile()}
        if set(source_payload) != source.paths:
            raise ValueError("git archive must preserve every tracked source file (review export-ignore/export-subst attributes)")
        overlap = set(support) & set(source_payload) - {"Cargo.lock"}
        if overlap:
            raise ValueError(f"Generated support files collide with tracked source: {sorted(overlap)}")
        source_payload.update(support)
        source_payload["RELEASE_CONTENTS.json"] = content_manifest(source_payload, tag, commit, "source")
        verify_payload(source_payload, tag, "source")
        buffer = io.BytesIO()
        with tarfile.open(fileobj=buffer, mode="w") as combined:
            for member in members:
                combined.addfile(member, archive.extractfile(member) if member.isfile() else None)
            for path, data in sorted(source_payload.items()):
                if path in source.paths:
                    continue
                info = tarfile.TarInfo(source_root + path)
                info.size, info.mtime, info.mode = len(data), epoch, 0o644
                combined.addfile(info, io.BytesIO(data))
        tar = buffer.getvalue()
    with (out / source_name).open("wb") as stream:
        with gzip.GzipFile(filename="", mode="wb", fileobj=stream, compresslevel=9, mtime=0) as compressed:
            compressed.write(tar)
    sums = []
    for name in sorted((web_name, source_name)):
        digest = hashlib.file_digest((out / name).open("rb"), "sha256").hexdigest()
        sums.append(f"{digest}  {name}\n")
        print(f"Packaged {name}: {(out / name).stat().st_size} bytes, SHA256 {digest}")
    (out / "SHA256SUMS").write_text("".join(sums), encoding="utf-8", newline="\n")
    verify_assets(out, tag)
    notes.parent.mkdir(parents=True, exist_ok=True)
    notes.write_text(release_notes(tag, commit, body, web_name, source_name), encoding="utf-8", newline="\n")
    print(f"Downloads: {out.relative_to(ROOT) if out.is_relative_to(ROOT) else out}")
    print(f"Release notes (not a download): {notes}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tag", help="Exact vMAJOR.MINOR.PATCH; required for packaging")
    parser.add_argument("--record-build", action="store_true", help="Record tool versions and asset hashes immediately after Trunk build")
    parser.add_argument("--verify-assets", help="Verify downloaded release archives/checksums without building or extracting")
    parser.add_argument("--check-only", action="store_true", help="Validate only; write no files")
    group = parser.add_mutually_exclusive_group()
    group.add_argument("--worktree", action="store_true", help="Check uncommitted working tree (check-only only)")
    group.add_argument("--rev", help="Git revision, defaults to the tag (or HEAD for tagless checks)")
    parser.add_argument("--dist", help="Trunk dist path; check-only checks dist only when explicitly supplied")
    parser.add_argument("--out", help="Empty download-assets directory (default target/release-assets/<tag>)")
    parser.add_argument("--notes", help="Release notes (default target/release-notes-<tag>.md)")
    args = parser.parse_args()
    if args.tag and not re.fullmatch("v" + SEMVER, args.tag):
        parser.error("--tag must be exactly vMAJOR.MINOR.PATCH")
    if sum((bool(args.verify_assets), args.record_build, args.check_only)) > 1:
        parser.error("--verify-assets, --record-build and --check-only are mutually exclusive")
    if args.verify_assets:
        try:
            verify_assets((ROOT / args.verify_assets).resolve(), args.tag)
            return 0
        except (ValueError, OSError, KeyError, zipfile.BadZipFile, tarfile.TarError, json.JSONDecodeError) as error:
            print(f"Release verification error: {error}", file=sys.stderr)
            return 1
    if args.record_build:
        try:
            build_provenance(Source("HEAD"), (ROOT / (args.dist or "crates/flood-web/dist")).resolve())
            return 0
        except (ValueError, OSError, subprocess.CalledProcessError, json.JSONDecodeError) as error:
            print(f"Build provenance error: {error}", file=sys.stderr)
            return 1
    if not args.check_only and (not args.tag or args.worktree):
        parser.error("Packaging requires --tag and a committed Git revision; --worktree is check-only")
    try:
        revision = None if args.worktree else args.rev or (f"refs/tags/{args.tag}" if args.tag else "HEAD")
        source = Source(revision)
        _version, body = validate_source(source, args.tag)
        files = validate_dist(source, (ROOT / (args.dist or "crates/flood-web/dist")).resolve()) if args.dist or not args.check_only else {}
        if args.check_only:
            print("Release checks passed (no files written)")
        else:
            out = (ROOT / (args.out or f"target/release-assets/{args.tag}")).resolve()
            notes = (ROOT / (args.notes or f"target/release-notes-{args.tag}.md")).resolve()
            dist = (ROOT / (args.dist or "crates/flood-web/dist")).resolve()
            if out == dist or dist in out.parents:
                raise ValueError("Output directory must not be inside dist")
            package(source, args.tag, body, files, out, notes)
        return 0
    except (ValueError, KeyError, OSError, tarfile.TarError, tomllib.TOMLDecodeError, json.JSONDecodeError) as error:
        print(f"Release error: {error}", file=sys.stderr)
        print("For uncommitted validation use --check-only --worktree; for pre-tag packaging use --rev HEAD.", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
