#!/usr/bin/env python3

import re
import sys
from pathlib import Path


def update_package_version(path: Path, version: str) -> None:
    content = path.read_text(encoding="utf-8")
    pattern = r'(?m)(^name = "apollo"\r?\nversion = ")[^"]+("$)'
    updated, replacements = re.subn(pattern, rf"\g<1>{version}\g<2>", content, count=1)
    if replacements != 1:
        raise SystemExit(f"could not find the apollo package version in {path}")
    path.write_text(updated, encoding="utf-8")


VERSION_RE = r"[0-9]{4}\.[0-9]{2}\.[0-9]{2}(-[0-9]+)?"


def main() -> None:
    if len(sys.argv) != 2 or not re.fullmatch(VERSION_RE, sys.argv[1]):
        raise SystemExit("usage: set-version.py YYYY.MM.DD[-N]")

    version = sys.argv[1]
    update_package_version(Path("Cargo.toml"), version)
    update_package_version(Path("Cargo.lock"), version)


if __name__ == "__main__":
    main()
