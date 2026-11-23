#!/usr/bin/env python3
"""Validate the rusqlite CI matrix or propose updates from crates.io releases."""

from __future__ import annotations

import argparse
import json
import pathlib
import re
import tomllib
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parent.parent
MANIFEST = pathlib.Path("wrapper/Cargo.toml")
MATRIX = pathlib.Path(".github/rusqlite-versions.json")
VERSION = re.compile(r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)")
Version = tuple[int, int, int]


def parse_version(value: str) -> Version:
    match = VERSION.fullmatch(value)
    if match is None:
        raise ValueError(f"not a stable release version: {value!r}")
    return tuple(map(int, match.groups()))


def bounds(requirement: str) -> tuple[Version, Version]:
    match = re.fullmatch(r">=([0-9.]+),\s*<([0-9.]+)", requirement)
    if match is None:
        raise ValueError(f"expected a bounded rusqlite range, found {requirement!r}")
    lower, upper = match.groups()
    if upper.count(".") == 1:
        upper += ".0"
    return parse_version(lower), parse_version(upper)


def load_configuration(root: pathlib.Path) -> tuple[str, list[str]]:
    manifest = tomllib.loads((root / MANIFEST).read_text())
    requirement = manifest["dependencies"]["rusqlite"]["version"]
    if manifest["dev-dependencies"]["rusqlite"]["version"] != requirement:
        raise ValueError("rusqlite dependency and dev-dependency ranges must match")
    lower, upper = bounds(requirement)
    versions = json.loads((root / MATRIX).read_text())
    if not isinstance(versions, list) or not versions:
        raise ValueError("the rusqlite matrix must be a nonempty list")
    parsed = [parse_version(value) for value in versions]
    if parsed != sorted(set(parsed)):
        raise ValueError("the rusqlite matrix must be sorted and unique")
    if parsed[0] != lower or any(not lower <= version < upper for version in parsed):
        raise ValueError("the rusqlite matrix must include the lower bound and stay in range")
    return requirement, versions


def plan_update(
    requirement: str, current: list[str], releases: list[dict]
) -> tuple[str, list[str]]:
    lower, upper = bounds(requirement)
    available = set()
    for release in releases:
        if release["yanked"] or VERSION.fullmatch(release["num"]) is None:
            continue
        version = parse_version(release["num"])
        if version >= lower:
            available.add(version)
    if not available:
        raise ValueError("crates.io returned no supported stable, non-yanked releases")

    latest = max(available)
    if latest >= upper:
        # Before 1.0, each minor release is a new compatibility line.
        upper = (0, latest[1] + 1, 0) if latest[0] == 0 else (latest[0] + 1, 0, 0)
        requirement = f">={'.'.join(map(str, lower))}, <{upper[0]}.{upper[1]}"

    # Keep the first and latest release of each minor line, including the floor.
    # Existing entries preserve coverage if an already-tested release is yanked.
    lines: dict[tuple[int, int], list[Version]] = {}
    for version in available | {parse_version(value) for value in current}:
        lines.setdefault(version[:2], []).append(version)
    selected = {version for line in lines.values() for version in (min(line), max(line))}
    return requirement, [".".join(map(str, version)) for version in sorted(selected)]


def update(root: pathlib.Path, releases: list[dict]) -> None:
    requirement, versions = load_configuration(root)
    new_requirement, new_versions = plan_update(requirement, versions, releases)
    manifest_path = root / MANIFEST
    manifest = manifest_path.read_text()
    if new_requirement != requirement:
        pattern = rf'(rusqlite = \{{ version = "){re.escape(requirement)}(")'
        manifest, count = re.subn(
            pattern, lambda match: match[1] + new_requirement + match[2], manifest
        )
        if count != 2:
            raise ValueError(f"expected two rusqlite dependency entries, found {count}")
        manifest_path.write_text(manifest)
    if new_versions != versions:
        (root / MATRIX).write_text(json.dumps(new_versions, indent=2) + "\n")
    load_configuration(root)
    print(f"rusqlite range: {new_requirement}; CI versions: {', '.join(new_versions)}")


def fetch_releases() -> list[dict]:
    request = urllib.request.Request(
        "https://crates.io/api/v1/crates/rusqlite",
        headers={
            "User-Agent": "rusqlite-derive compatibility monitor "
            "(https://github.com/Cerber-Ursi/rusqlite-derive)"
        },
    )
    with urllib.request.urlopen(request, timeout=30) as response:
        return json.load(response)["versions"]


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("matrix", "update"))
    args = parser.parse_args()
    if args.command == "matrix":
        _, versions = load_configuration(ROOT)
        print(json.dumps(versions, separators=(",", ":")))
    else:
        update(ROOT, fetch_releases())


if __name__ == "__main__":
    main()
