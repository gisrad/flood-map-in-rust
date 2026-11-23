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
import shutil
import subprocess
import sys
import tarfile
import time
from urllib.parse import unquote, urlsplit
import zipfile

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
    "crates/flood-web/Trunk.toml", "crates/flood-web/index.html",
    "crates/flood-worker/wrangler.toml.example",
    "scripts/package_release.py", "scripts/check_wasm_size.mjs",
)
GENERATED_DIRS = {
    "target", "dist", "build", "node_modules", ".git", ".wrangler", "__pycache__",
    "test-results", "playwright-report", "blob-report", "results", "report",
}


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


def validate_dist(source: Source, dist: Path) -> dict[str, Path]:
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
                copied += 1
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
dan OFL font. Statistik turunan GSIM tetap CC BY-SA 3.0.
SHA256SUMS memeriksa integritas unduhan, bukan autentikasi penerbit. Tidak ada
klaim tanda tangan, attestation penerbit, atau SBOM lengkap.
"""


def package(source: Source, tag: str, body: str, files: dict[str, Path], out: Path, notes: Path) -> None:
    commit = source.commit
    if git("rev-parse", "HEAD").decode().strip() != commit:
        raise ValueError("Checkout HEAD must equal the packaged revision; rebuild dist from that checkout")
    if git("diff", "--name-only", commit, "--"):
        raise ValueError("Tracked working-tree changes present; commit changes and rebuild before packaging")
    if out == notes.parent or out in notes.parents:
        raise ValueError("Release notes must be outside the download-assets directory")
    out.mkdir(parents=True, exist_ok=True)
    web_name = f"{PROJECT}-{tag}-web.zip"
    source_name = f"{PROJECT}-{tag}-source.tar.gz"
    allowed = {web_name, source_name, "SHA256SUMS"}
    if any(p.name not in allowed for p in out.iterdir()):
        raise ValueError(f"Output directory contains unrelated/stale assets: {out}")
    epoch = int(git("show", "-s", "--format=%ct", commit).strip())
    zip_time = time.gmtime(max(315532800, min(epoch, 4354819198)))[:6]
    root = f"{PROJECT}-{tag}-web"
    entries = {f"{root}/{p}": source.read(p) for p in DOCS}
    entries[f"{root}/RELEASE_NOTICE.txt"] = release_notice(tag, commit).encode("utf-8")
    entries.update({f"{root}/site/{p}": f.read_bytes() for p, f in files.items()})
    with zipfile.ZipFile(out / web_name, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for path, data in sorted(entries.items()):
            info = zipfile.ZipInfo(path, zip_time)
            info.create_system = 3
            info.external_attr = 0o100644 << 16
            info.compress_type = zipfile.ZIP_DEFLATED
            archive.writestr(info, data, compresslevel=9)
    # Pure git archive: only tracked source, including lockfiles/workflows/test code.
    # The tracked docs/RELEASING.md is the source archive's release notice.
    tar = git("archive", "--format=tar", f"--prefix={PROJECT}-{tag}/", commit)
    with tarfile.open(fileobj=io.BytesIO(tar)) as archive:
        names = {m.name for m in archive.getmembers() if m.isfile()}
        for path in REQUIRED:
            if f"{PROJECT}-{tag}/{path}" not in names:
                raise ValueError(f"git archive omitted required file (check export-ignore): {path}")
        if not any(p.startswith(f"{PROJECT}-{tag}/tests/e2e/") for p in names):
            raise ValueError("git archive omitted e2e test code")
    with (out / source_name).open("wb") as stream:
        with gzip.GzipFile(filename="", mode="wb", fileobj=stream, compresslevel=9, mtime=0) as compressed:
            compressed.write(tar)
    sums = []
    for name in sorted((web_name, source_name)):
        digest = hashlib.file_digest((out / name).open("rb"), "sha256").hexdigest()
        sums.append(f"{digest}  {name}\n")
        print(f"Packaged {name}: {(out / name).stat().st_size} bytes, SHA256 {digest}")
    (out / "SHA256SUMS").write_text("".join(sums), encoding="utf-8", newline="\n")
    notes.parent.mkdir(parents=True, exist_ok=True)
    notes.write_text(
        f"> **{DISCLAIMER}.** Ikuti informasi resmi BPBD/BNPB, BMKG dan pengelola sungai.\n\n"
        f"Tag: `{tag}`\n\nCommit: `{commit}`\n\n{body}\n\n"
        f"## Unduhan\n\n- `{web_name}`: frontend statis (`site/`), lisensi dan dokumen. **Backend /api/* tidak dibundel**; perlu backend kompatibel pada origin yang sama.\n"
        f"- `{source_name}`: sumber Git, lockfile, workflow dan kode tes; bukan installer.\n"
        "- `SHA256SUMS`: jalankan `sha256sum -c SHA256SUMS` dari direktori unduhan. Checksum bukan autentikasi penerbit; belum ada tanda tangan/attestation atau SBOM lengkap.\n\n"
        "Lisensi kode MIT tidak mengganti lisensi data/pustaka. Baca DATA_LICENSES.md dan THIRD_PARTY_NOTICES.md; GSIM tetap CC BY-SA 3.0. Rilis ini tidak melakukan deploy produksi.\n",
        encoding="utf-8", newline="\n",
    )
    print(f"Downloads: {out.relative_to(ROOT) if out.is_relative_to(ROOT) else out}")
    print(f"Release notes (not a download): {notes}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tag", help="Exact vMAJOR.MINOR.PATCH; required for packaging")
    parser.add_argument("--check-only", action="store_true", help="Validate only; write no files")
    group = parser.add_mutually_exclusive_group()
    group.add_argument("--worktree", action="store_true", help="Check uncommitted working tree (check-only only)")
    group.add_argument("--rev", help="Git revision, defaults to the tag (or HEAD for tagless checks)")
    parser.add_argument("--dist", help="Trunk dist path; check-only checks dist only when explicitly supplied")
    parser.add_argument("--out", default="target/release-assets", help="Download-assets directory")
    parser.add_argument("--notes", default="target/release-notes.md", help="Release notes outside assets directory")
    args = parser.parse_args()
    if args.tag and not re.fullmatch("v" + SEMVER, args.tag):
        parser.error("--tag must be exactly vMAJOR.MINOR.PATCH")
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
            out, notes = (ROOT / args.out).resolve(), (ROOT / args.notes).resolve()
            dist = (ROOT / (args.dist or "crates/flood-web/dist")).resolve()
            if out == dist or dist in out.parents:
                raise ValueError("Output directory must not be inside dist")
            package(source, args.tag, body, files, out, notes)
        return 0
    except (ValueError, KeyError, OSError, tomllib.TOMLDecodeError, json.JSONDecodeError) as error:
        print(f"Release error: {error}", file=sys.stderr)
        print("For uncommitted validation use --check-only --worktree; for pre-tag packaging use --rev HEAD.", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
