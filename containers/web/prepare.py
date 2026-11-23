"""Prepare a Docker context from a verified source/web release, never a local dist."""

import argparse
import base64
import hashlib
from pathlib import Path
import re
import shutil
import subprocess
import sys
import zipfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--assets", type=Path, required=True)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    if not re.fullmatch(r"v\d+\.\d+\.\d+", args.tag):
        parser.error("--tag must be vMAJOR.MINOR.PATCH")
    if args.out.exists() and any(args.out.iterdir()):
        parser.error("--out must be empty or new")

    here = Path(__file__).resolve().parent
    subprocess.run([
        sys.executable, str(here.parents[1] / "scripts/package_release.py"),
        "--verify-assets", str(args.assets.resolve()), "--tag", args.tag,
    ], check=True)
    args.out.mkdir(parents=True, exist_ok=True)
    root = f"flood-map-in-rust-{args.tag}-web"
    with zipfile.ZipFile(args.assets / f"{root}.zip") as archive:
        # The existing verifier has checked all member paths, types and hashes.
        archive.extractall(args.out)
    release = args.out / "release"
    (args.out / root).rename(release)
    html = (release / "site/index.html").read_bytes().decode("utf-8")
    hashes = []
    for attrs, body in re.findall(r"<script\b([^>]*)>(.*?)</script\s*>", html, re.I | re.S):
        if re.search(r"\bsrc\s*=", attrs, re.I) or "application/ld+json" in attrs.lower():
            continue
        digest = base64.b64encode(hashlib.sha256(body.encode("utf-8")).digest()).decode("ascii")
        hashes.append(f"'sha256-{digest}'")
    if not hashes:
        raise ValueError("No Trunk inline loader found in verified HTML")
    config = (here / "default.conf.in").read_text(encoding="utf-8")
    (args.out / "default.conf").write_text(
        config.replace("@@SCRIPT_HASHES@@", " ".join(hashes)), encoding="utf-8", newline="\n",
    )
    shutil.copyfile(here / "Dockerfile", args.out / "Dockerfile")
    print(f"Prepared {args.out} from {args.tag}; API upstream remains flood.transbulent.id")


if __name__ == "__main__":
    main()
