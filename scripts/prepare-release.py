#!/usr/bin/env python3
"""Prepare, optionally push, a stable release from the master branch."""

from __future__ import annotations

import argparse
import datetime
import json
import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
STABLE_VERSION = re.compile(r"^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$")


def run(*args: str, capture: bool = False) -> str:
    result = subprocess.run(
        args,
        cwd=ROOT,
        check=True,
        text=True,
        stdout=subprocess.PIPE if capture else None,
    )
    return result.stdout.rstrip("\n") if capture else ""


def replace_once(text: str, old: str, new: str, path: pathlib.Path) -> str:
    count = text.count(old)
    if count != 1:
        raise ValueError(f"expected exactly one {old!r} in {path}, found {count}")
    return text.replace(old, new)


def package_versions() -> dict[str, str]:
    metadata = json.loads(
        run("cargo", "metadata", "--no-deps", "--format-version", "1", capture=True)
    )
    names = {"rusqlite-derive", "rusqlite-derive-impl"}
    return {
        package["name"]: package["version"]
        for package in metadata["packages"]
        if package["name"] in names
    }


def update_changelog(text: str, old_version: str, new_version: str) -> str:
    if re.search(rf"^## \[{re.escape(new_version)}\](?: - .+)?$", text, re.MULTILINE):
        raise ValueError(f"CHANGELOG.md already has a section for {new_version}")

    heading = re.search(r"^## \[Unreleased\]$", text, re.MULTILINE)
    if heading is None:
        raise ValueError("CHANGELOG.md has no Unreleased section")
    next_heading = re.search(r"^## ", text[heading.end() :], re.MULTILINE)
    if next_heading is None:
        raise ValueError("CHANGELOG.md has no released section after Unreleased")

    released_heading_start = heading.end() + next_heading.start()
    expected_previous_heading = re.compile(
        rf"^## \[{re.escape(old_version)}\](?: - .+)?$", re.MULTILINE
    )
    previous_heading = expected_previous_heading.match(text, released_heading_start)
    if previous_heading is None:
        raise ValueError(
            f"the first released CHANGELOG.md section is not {old_version}"
        )

    entries = text[heading.end() : released_heading_start].strip()
    if not entries:
        raise ValueError("CHANGELOG.md Unreleased section is empty")

    date = datetime.datetime.now(datetime.timezone.utc).date().isoformat()
    replacement = (
        f"## [Unreleased]\n\n"
        f"## [{new_version}] - {date}\n\n"
        f"{entries}\n\n"
    )
    text = text[: heading.start()] + replacement + text[released_heading_start:]

    unreleased_link = re.compile(r"^\[Unreleased\]: .+$", re.MULTILINE)
    matches = list(unreleased_link.finditer(text))
    if len(matches) != 1:
        raise ValueError(
            f"expected exactly one Unreleased comparison link, found {len(matches)}"
        )
    repository = "https://github.com/Cerber-Ursi/rusqlite-derive"
    links = (
        f"[Unreleased]: {repository}/compare/v{new_version}...HEAD\n"
        f"[{new_version}]: {repository}/compare/v{old_version}...v{new_version}"
    )
    return unreleased_link.sub(links, text, count=1)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="update versions and CHANGELOG.md, then commit and tag a release"
    )
    parser.add_argument("version", help="stable semantic version, without a leading v")
    parser.add_argument(
        "--push",
        action="store_true",
        help="atomically push master and the release tag to the selected remote",
    )
    parser.add_argument("--remote", default="origin", help="Git remote (default: origin)")
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    match = STABLE_VERSION.fullmatch(args.version)
    if match is None:
        raise SystemExit(f"invalid stable semantic version: {args.version!r}")
    new_version_tuple = tuple(map(int, match.groups()))
    tag = f"v{args.version}"

    if run("git", "status", "--porcelain", capture=True):
        raise SystemExit("the working tree must be clean")
    branch = run("git", "branch", "--show-current", capture=True)
    if branch != "master":
        raise SystemExit(f"releases must be prepared from master, not {branch!r}")

    run("git", "fetch", "--quiet", "--tags", args.remote)
    remote_head = run("git", "rev-parse", f"refs/remotes/{args.remote}/master", capture=True)
    local_head = run("git", "rev-parse", "HEAD", capture=True)
    if local_head != remote_head:
        raise SystemExit(f"master must exactly match {args.remote}/master")
    if subprocess.run(
        ["git", "rev-parse", "--verify", "--quiet", f"refs/tags/{tag}"],
        cwd=ROOT,
    ).returncode == 0:
        raise SystemExit(f"tag {tag} already exists")

    versions = package_versions()
    expected_packages = {"rusqlite-derive", "rusqlite-derive-impl"}
    if set(versions) != expected_packages:
        raise SystemExit(f"could not find both release packages: {versions}")
    current_versions = set(versions.values())
    if len(current_versions) != 1:
        raise SystemExit(f"release package versions differ: {versions}")
    old_version = current_versions.pop()
    old_match = STABLE_VERSION.fullmatch(old_version)
    if old_match is None:
        raise SystemExit(f"current package version is not stable semver: {old_version}")
    if new_version_tuple <= tuple(map(int, old_match.groups())):
        raise SystemExit(f"new version {args.version} must be newer than {old_version}")

    workspace_manifest = ROOT / "Cargo.toml"
    wrapper_manifest = ROOT / "wrapper" / "Cargo.toml"
    changelog_path = ROOT / "CHANGELOG.md"

    workspace = replace_once(
        workspace_manifest.read_text(),
        f'version = "{old_version}"',
        f'version = "{args.version}"',
        workspace_manifest,
    )
    wrapper = replace_once(
        wrapper_manifest.read_text(),
        f'version = "={old_version}", path = "../impl"',
        f'version = "={args.version}", path = "../impl"',
        wrapper_manifest,
    )
    changelog = update_changelog(
        changelog_path.read_text(), old_version, args.version
    )

    workspace_manifest.write_text(workspace)
    wrapper_manifest.write_text(wrapper)
    changelog_path.write_text(changelog)

    run("cargo", "check", "--workspace", "--all-targets", "--all-features")
    updated_versions = package_versions()
    if set(updated_versions.values()) != {args.version}:
        raise SystemExit(f"updated package versions are inconsistent: {updated_versions}")
    run("cargo", "fmt", "--all", "--", "--check")
    run(
        "cargo",
        "clippy",
        "--workspace",
        "--all-targets",
        "--all-features",
        "--",
        "-D",
        "warnings",
    )
    run("cargo", "test", "--workspace", "--all-targets", "--all-features")
    run("cargo", "test", "--workspace", "--doc", "--all-features")

    changed_paths = {
        line[3:]
        for line in run("git", "status", "--porcelain", capture=True).splitlines()
    }
    expected_paths = {
        "Cargo.lock",
        "Cargo.toml",
        "CHANGELOG.md",
        "wrapper/Cargo.toml",
    }
    if changed_paths != expected_paths:
        raise SystemExit(
            f"release preparation changed unexpected paths: {sorted(changed_paths)}"
        )

    run(
        "git",
        "add",
        "Cargo.toml",
        "Cargo.lock",
        "wrapper/Cargo.toml",
        "CHANGELOG.md",
    )
    run("git", "commit", "-m", f"release: prepare {args.version}")
    run("git", "tag", "--annotate", tag, "--message", f"Release {args.version}")

    if args.push:
        run("git", "push", "--atomic", args.remote, "master", tag)
        print(f"prepared and pushed {tag}")
    else:
        print(f"prepared {tag} locally; inspect it, then run:")
        print(f"  git push --atomic {args.remote} master {tag}")


if __name__ == "__main__":
    try:
        main()
    except (subprocess.CalledProcessError, ValueError) as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(1)
