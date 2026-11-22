#!/usr/bin/env python3
"""Portable source checks; these do not compile or type-check Rust."""

import ast
import json
import re
from pathlib import Path
import sys
import tomllib
import xml.etree.ElementTree as ET


def main():
    root = Path(__file__).resolve().parents[1]
    workspace = tomllib.loads((root / "Cargo.toml").read_text())
    shared = workspace["workspace"]["dependencies"]
    members = workspace["workspace"]["members"]
    package_names = set()
    for member in members:
        manifest = root / member / "Cargo.toml"
        data = tomllib.loads(manifest.read_text())
        name = data["package"]["name"]
        if name in package_names:
            raise ValueError(f"Duplicate workspace package: {name}")
        package_names.add(name)
        for section in ("dependencies", "dev-dependencies", "build-dependencies"):
            for dependency, options in data.get(section, {}).items():
                if isinstance(options, dict) and options.get("workspace"):
                    if dependency not in shared:
                        raise ValueError(f"Undefined workspace dependency: {dependency}")
                if isinstance(options, dict) and "path" in options:
                    target = manifest.parent / options["path"] / "Cargo.toml"
                    if not target.is_file():
                        raise ValueError(f"Missing local dependency: {target}")
        if name in {"diamond-core", "diamond-document", "diamond-vault"}:
            gui = {"egui", "eframe", "diamond-app", "diamond-editor"}
            forbidden = gui.intersection(data.get("dependencies", {}))
            if forbidden:
                raise ValueError(f"GUI dependency violates {name} boundary: {forbidden}")
    for name, dependency in shared.items():
        if isinstance(dependency, dict) and "path" in dependency:
            if not (root / dependency["path"] / "Cargo.toml").is_file():
                raise ValueError(f"Missing workspace path: {name}")

    counts = {"toml": 0, "json": 0, "python": 0, "svg": 0}
    for path in root.rglob("*"):
        if not path.is_file() or any(part in {"target", ".git", "__pycache__"} for part in path.parts):
            continue
        if path.suffix == ".toml":
            tomllib.loads(path.read_text())
            counts["toml"] += 1
        elif path.suffix == ".json":
            json.loads(path.read_text())
            counts["json"] += 1
        elif path.suffix == ".py":
            ast.parse(path.read_text(), filename=str(path))
            counts["python"] += 1
        elif path.suffix == ".svg":
            ET.parse(path)
            counts["svg"] += 1
        elif path.suffix == ".rs":
            for target in re.findall(r'include_(?:bytes|str)!\(\s*"([^"]+)"', path.read_text()):
                if not (path.parent / target).is_file():
                    raise ValueError(f"Missing embedded asset: {path}: {target}")
    if not (root / "assets/lucide/LICENSE").is_file():
        raise ValueError("Bundled Lucide license is missing")
    print(f"PASS: {len(members)} workspace packages, local paths and GUI boundaries; {counts}")
    print("Rust compilation, formatting, tests and resolved dependency graph were not checked.")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError, KeyError, SyntaxError, ET.ParseError) as error:
        print(f"FAIL: source checks: {error}", file=sys.stderr)
        raise SystemExit(1)
