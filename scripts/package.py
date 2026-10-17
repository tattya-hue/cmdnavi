"""Package a native release build; Python 3.11+ with no third-party dependencies."""
import argparse
import hashlib
import os
from pathlib import Path
import subprocess
import tarfile
import tomllib
import zipfile

ROOT = Path(__file__).resolve().parents[1]
TARGETS = {
    "x86_64-unknown-linux-gnu": "cmdnavi",
    "x86_64-pc-windows-msvc": "cmdnavi.exe",
}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", choices=TARGETS)
    parser.add_argument("--check-version", action="store_true")
    args = parser.parse_args()
    version = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))["package"]["version"]
    if os.environ.get("GITHUB_REF_TYPE") == "tag":
        expected = f"v{version}"
        actual = os.environ.get("GITHUB_REF_NAME")
        if actual != expected:
            parser.error(f"Tag {actual!r} must match package version {expected!r}")
    if args.check_version:
        print(f"Package version: {version}")
        return
    if not args.target:
        parser.error("--target is required for packaging")
    binary = ROOT / "target" / args.target / "release" / TARGETS[args.target]
    actual = subprocess.check_output([binary, "--version"], text=True).strip()
    if actual != f"cmdnavi {version}":
        parser.error(f"Executable version mismatch: {actual!r}")
    name = f"cmdnavi-v{version}-{args.target}"
    output = ROOT / "dist"
    output.mkdir(exist_ok=True)
    files = [binary, ROOT / "README.md", ROOT / "LICENSE", ROOT / "RELEASE_NOTES.md"]
    files += sorted((ROOT / "completions").iterdir())
    files += [ROOT / "docs" / "completion.md"]

    def member(file):
        if file.parent.name in {"completions", "docs"}:
            return f"{name}/{file.parent.name}/{file.name}"
        return f"{name}/{file.name}"

    if args.target.endswith("windows-msvc"):
        archive = output / f"{name}.zip"
        with zipfile.ZipFile(archive, "w", compression=zipfile.ZIP_DEFLATED) as package:
            for file in files:
                package.write(file, member(file))
    else:
        archive = output / f"{name}.tar.gz"
        with tarfile.open(archive, "w:gz") as package:
            for file in files:
                package.add(file, arcname=member(file))
    digest = hashlib.sha256(archive.read_bytes()).hexdigest()
    archive.with_name(archive.name + ".sha256").write_text(f"{digest}  {archive.name}\n", encoding="utf-8", newline="\n")
    print(archive)


if __name__ == "__main__":
    main()
