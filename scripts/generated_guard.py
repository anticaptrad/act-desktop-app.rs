#!/usr/bin/env python3
"""Check, freeze, or unfreeze a repository's root generated/ tree.

This tool deliberately separates:
- configuration projections sourced from .cli-flags.toml;
- contract projections sourced from independent TypeSpec and JSON Schema/OpenAPI;
- generated API documentation.

Git does not persist Unix write bits, so `check --require-frozen` is a local
working-tree assertion. Deterministic regeneration plus a clean git diff is the
durable CI control.
"""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import stat
import sys
import tomllib
from typing import Any, Iterable

CODE_SUFFIXES = {
    ".c", ".cc", ".cpp", ".cs", ".dart", ".ex", ".exs", ".gleam",
    ".go", ".h", ".hpp", ".java", ".js", ".jsx", ".kt", ".kts",
    ".php", ".py", ".rb", ".rs", ".swift", ".ts", ".tsx", ".zig",
}
GENERATED_MARKERS = (
    "generated",
    "do not edit",
    "do not hand-edit",
    "@generated",
)


class PolicyError(RuntimeError):
    """A generated-tree policy violation."""


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("command", choices=("check", "freeze", "unfreeze"))
    parser.add_argument("--root", type=Path, default=Path("."))
    parser.add_argument("--generated", type=Path, default=Path("generated"))
    parser.add_argument("--require-frozen", action="store_true")
    parser.add_argument("--require-manifest", action="store_true")
    return parser.parse_args()


def generated_root(args: argparse.Namespace) -> tuple[Path, Path]:
    repo = args.root.resolve()
    generated = args.generated
    if not generated.is_absolute():
        generated = repo / generated
    return repo, generated.resolve()


def iter_tree(path: Path) -> Iterable[Path]:
    # Do not follow symlinks into untrusted or cyclic trees.
    for root, dirs, files in os.walk(path, followlinks=False):
        current = Path(root)
        for name in sorted(dirs):
            yield current / name
        for name in sorted(files):
            yield current / name


def relative(path: Path, repo: Path) -> str:
    try:
        return path.relative_to(repo).as_posix()
    except ValueError:
        return str(path)


def ensure_within_repo(repo: Path, generated: Path) -> None:
    try:
        generated.relative_to(repo)
    except ValueError as exc:
        raise PolicyError(f"generated path escapes repository: {generated}") from exc


def load_manifest(generated: Path, require_manifest: bool) -> dict[str, Any] | None:
    path = generated / "manifest.json"
    if not path.exists():
        if require_manifest:
            raise PolicyError("generated/manifest.json is required")
        return None
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        raise PolicyError(f"could not parse {path}: {exc}") from exc
    if not isinstance(value, dict):
        raise PolicyError("generated/manifest.json must be an object")
    required = (
        "schemaVersion", "classification", "authorities", "generators",
        "outputs", "peerContractAuthority", "permissions",
    )
    missing = [key for key in required if key not in value]
    if missing:
        raise PolicyError(f"generated/manifest.json missing: {', '.join(missing)}")
    if value["schemaVersion"] != 1:
        raise PolicyError("generated manifest schemaVersion must be 1")
    return value


def authority_paths(repo: Path, manifest: dict[str, Any] | None) -> list[Path]:
    if manifest is None:
        return []
    result: list[Path] = []
    for item in manifest.get("authorities", []):
        if not isinstance(item, dict) or not isinstance(item.get("path"), str):
            raise PolicyError("each authority must contain a string path")
        path = (repo / item["path"]).resolve()
        try:
            path.relative_to(repo)
        except ValueError as exc:
            raise PolicyError(f"authority escapes repository: {item['path']}") from exc
        if not path.exists():
            raise PolicyError(f"declared authority does not exist: {item['path']}")
        result.append(path)
    return result


def verify_output_scope(generated: Path, manifest: dict[str, Any] | None) -> None:
    if manifest is None:
        return
    for name in manifest.get("forbiddenOutputDirectories", []):
        if not isinstance(name, str) or "/" in name or "\\" in name:
            raise PolicyError(f"invalid forbidden output directory: {name!r}")
        if (generated / name).exists():
            raise PolicyError(f"forbidden generated output is present: generated/{name}")

    for item in manifest.get("outputs", []):
        if not isinstance(item, dict) or not isinstance(item.get("path"), str):
            raise PolicyError("each output must contain a string path")
        path = (generated.parent / item["path"]).resolve()
        try:
            path.relative_to(generated.parent)
        except ValueError as exc:
            raise PolicyError(f"output escapes repository: {item['path']}") from exc
        if not path.exists():
            raise PolicyError(f"declared generated output does not exist: {item['path']}")


def collect_flag_tables(table: dict[str, Any]) -> list[dict[str, Any]]:
    result: list[dict[str, Any]] = []
    flags = table.get("flags")
    if isinstance(flags, dict):
        result.extend(value for value in flags.values() if isinstance(value, dict))
    commands = table.get("commands")
    if isinstance(commands, dict):
        for command in commands.values():
            if isinstance(command, dict):
                result.extend(collect_flag_tables(command))
    return result


def verify_config_projection(repo: Path, generated: Path, manifest: dict[str, Any] | None) -> None:
    classification = manifest.get("classification") if manifest else None
    marker_found = False
    for candidate in (generated / "rust" / "env.rs", generated / "typescript" / "env.ts",
                      generated / "dart" / "env.dart", generated / "gleam" / "env.gleam"):
        if candidate.exists():
            text = candidate.read_text(encoding="utf-8", errors="replace").lower()
            marker_found = "flags-2-env" in text
            if marker_found:
                break
    is_config = classification in {
        "configuration_projection",
        "mixed_configuration_and_api_docs_projection",
    } or marker_found
    if not is_config:
        return

    catalog_path = repo / ".cli-flags.toml"
    if not catalog_path.exists():
        raise PolicyError("flags-2-env output exists but .cli-flags.toml is missing")
    try:
        catalog = tomllib.loads(catalog_path.read_text(encoding="utf-8"))
    except (OSError, tomllib.TOMLDecodeError) as exc:
        raise PolicyError(f"could not parse .cli-flags.toml: {exc}") from exc

    keys: list[str] = []
    for flag in collect_flag_tables(catalog):
        env = flag.get("env")
        if isinstance(env, str) and env.strip():
            keys.append(env.strip())
    if not keys:
        raise PolicyError(".cli-flags.toml has no flags with env keys")

    generated_text = "\n".join(
        path.read_text(encoding="utf-8", errors="replace")
        for path in iter_tree(generated)
        if path.is_file() and path.suffix in CODE_SUFFIXES.union({".json"})
    )
    absent = sorted({key for key in keys if key not in generated_text})
    if absent:
        raise PolicyError(
            "catalog env keys absent from generated outputs: " + ", ".join(absent)
        )


def verify_generated_markers(generated: Path) -> None:
    violations: list[str] = []
    for path in iter_tree(generated):
        if not path.is_file() or path.name in {"README.md", "readme.md", "manifest.json"}:
            continue
        if path.suffix not in CODE_SUFFIXES:
            continue
        prefix = path.read_text(encoding="utf-8", errors="replace")[:4096].lower()
        if not any(marker in prefix for marker in GENERATED_MARKERS):
            violations.append(path.relative_to(generated).as_posix())
    if violations:
        raise PolicyError(
            "generated code missing a generated/do-not-edit marker: "
            + ", ".join(violations)
        )


def writable_bits(path: Path) -> int:
    return stat.S_IMODE(path.lstat().st_mode) & 0o222


def verify_frozen(generated: Path) -> None:
    writable = [
        path.relative_to(generated.parent).as_posix()
        for path in [generated, *iter_tree(generated)]
        if not path.is_symlink() and writable_bits(path)
    ]
    if writable:
        raise PolicyError(
            "generated tree is locally writable: " + ", ".join(writable)
        )


def check(repo: Path, generated: Path, require_frozen: bool, require_manifest: bool) -> None:
    ensure_within_repo(repo, generated)
    if not generated.exists():
        print(f"no generated root: {relative(generated, repo)}")
        return
    if not generated.is_dir() or generated.is_symlink():
        raise PolicyError("generated path must be a real directory, not a symlink")
    readmes = [
        path
        for path in (generated / "readme.md", generated / "README.md")
        if path.is_file()
    ]
    if not readmes:
        raise PolicyError("generated/readme.md is required")
    if len(readmes) != 1:
        raise PolicyError("generated/ must contain exactly one readme.md/README.md notice")
    readme_text = readmes[0].read_text(encoding="utf-8", errors="replace").lower()
    if "do not" not in readme_text or "edit" not in readme_text:
        raise PolicyError("generated/readme.md must explicitly prohibit direct edits")

    manifest = load_manifest(generated, require_manifest)
    authority_paths(repo, manifest)
    verify_output_scope(generated, manifest)
    verify_config_projection(repo, generated, manifest)
    verify_generated_markers(generated)
    if require_frozen:
        verify_frozen(generated)
    print(f"generated policy check passed: {relative(generated, repo)}")


def freeze(generated: Path) -> None:
    if not generated.exists():
        print(f"nothing to freeze: {generated}")
        return
    paths = [path for path in iter_tree(generated) if not path.is_symlink()]
    # Freeze children first, then directories from deepest to shallowest.
    files = [path for path in paths if path.is_file()]
    directories = sorted(
        [path for path in paths if path.is_dir()],
        key=lambda path: len(path.parts),
        reverse=True,
    )
    for path in files:
        os.chmod(path, stat.S_IMODE(path.stat().st_mode) & ~0o222)
    for path in directories:
        os.chmod(path, stat.S_IMODE(path.stat().st_mode) & ~0o222)
    os.chmod(generated, stat.S_IMODE(generated.stat().st_mode) & ~0o222)
    print(f"froze generated tree: {generated}")


def unfreeze(generated: Path) -> None:
    if not generated.exists():
        print(f"nothing to unfreeze: {generated}")
        return
    # Open parents before children.
    os.chmod(generated, stat.S_IMODE(generated.stat().st_mode) | 0o700)
    for path in iter_tree(generated):
        if path.is_symlink():
            continue
        mode = stat.S_IMODE(path.stat().st_mode)
        os.chmod(path, mode | (0o700 if path.is_dir() else 0o600))
    print(f"unfroze generated tree: {generated}")


def main() -> int:
    args = parse_args()
    repo, generated = generated_root(args)
    try:
        if args.command == "check":
            check(repo, generated, args.require_frozen, args.require_manifest)
        elif args.command == "freeze":
            freeze(generated)
        else:
            unfreeze(generated)
    except (OSError, PolicyError) as exc:
        print(f"generated policy error: {exc}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
