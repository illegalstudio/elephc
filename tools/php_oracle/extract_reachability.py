#!/usr/bin/env python3
"""Derive PHP stream consumers from an exact configured php-src C build."""

from __future__ import annotations

import argparse
import concurrent.futures
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
from collections import defaultdict, deque
from pathlib import Path
from typing import Any

PHP_RELEASE = "8.5.6"
PHP_SRC_COMMIT = "fcc29c8d6d6ee6f5ba2d941f0a2a6ea6aa6ee633"
SCHEMA_VERSION = 1
ROOT = Path(__file__).resolve().parents[2]
SYMBOL = r'(?:"(?P<quoted>(?:[^"\\]|\\.)+)"|(?P<plain>[A-Za-z$._][A-Za-z0-9$._-]*))'
DEFINE_RE = re.compile(rf"^define\s+(?P<prefix>.*?)@{SYMBOL}\(")
CALL_RE = re.compile(
    rf"\b(?:call|invoke)\b.*?(?P<target>@{SYMBOL}|%[A-Za-z$._][A-Za-z0-9$._-]*)\("
)
INCLUDE_RE = re.compile(r'^\s*#\s*include\s+"([^"]+_arginfo\.h)"', re.MULTILINE)
CLASS_TABLE_RE = re.compile(
    r"static\s+const\s+zend_function_entry\s+class_([A-Za-z0-9_]+)_methods"
)
ROOT_RE = re.compile(
    r"(?:^|_)(?:php_)?stream(?:_|$)|"
    r"(?:wrapper|filter|bucket|brigade|transport|xport|context).*stream",
    re.IGNORECASE,
)


def parse_args() -> argparse.Namespace:
    """Parse reachability extraction and checked-in verification arguments."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--compile-log", required=True, type=Path)
    parser.add_argument("--php-src", required=True, type=Path)
    parser.add_argument("--build-dir", required=True, type=Path)
    parser.add_argument("--target", required=True)
    parser.add_argument("--build-profile", required=True)
    parser.add_argument("--clang", type=Path)
    parser.add_argument("--directives-preprocessor", type=Path)
    parser.add_argument("--jobs", type=int, default=min(4, os.cpu_count() or 1))
    parser.add_argument("--output", type=Path)
    action = parser.add_mutually_exclusive_group(required=True)
    action.add_argument("--write", action="store_true")
    action.add_argument("--check", action="store_true")
    return parser.parse_args()


def canonical_bytes(value: Any) -> bytes:
    """Serialize deterministic UTF-8 JSON with sorted keys and a final LF."""
    return (
        json.dumps(value, indent=2, sort_keys=True, ensure_ascii=False) + "\n"
    ).encode()


def sha256_bytes(content: bytes) -> str:
    """Return the lowercase SHA-256 digest for arbitrary bytes."""
    return hashlib.sha256(content).hexdigest()


def sha256_file(path: Path) -> str:
    """Return the SHA-256 digest for one file."""
    return sha256_bytes(path.read_bytes())


def git_value(repo: Path, expression: str) -> str:
    """Resolve one Git revision expression in a checkout."""
    result = subprocess.run(
        ["git", "-C", str(repo), "rev-parse", expression],
        check=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    return result.stdout.strip()


def normalize_symbol(quoted: str | None, plain: str | None) -> str:
    """Normalize one LLVM symbol, including Apple's escaped assembler prefix."""
    symbol = quoted if quoted is not None else plain
    assert symbol is not None
    if symbol.startswith("\\01"):
        symbol = symbol[3:]
    if symbol.startswith("\x01"):
        symbol = symbol[1:]
    return symbol


def source_argument(arguments: list[str]) -> Path | None:
    """Return the translation unit following `-c`, if this is a C compile."""
    if "-c" not in arguments:
        return None
    index = arguments.index("-c")
    if index + 1 >= len(arguments):
        return None
    source = Path(arguments[index + 1])
    return source if source.suffix == ".c" else None


def load_compile_records(path: Path) -> list[dict[str, Any]]:
    """Load and deduplicate successful-looking C translation-unit captures."""
    records: dict[tuple[str, tuple[str, ...]], dict[str, Any]] = {}
    for line_number, line in enumerate(path.read_text().splitlines(), start=1):
        try:
            record = json.loads(line)
        except json.JSONDecodeError as error:
            raise SystemExit(
                f"invalid compiler JSONL at line {line_number}: {error}"
            ) from error
        arguments = [str(value) for value in record.get("arguments", [])]
        source = source_argument(arguments)
        if source is None:
            continue
        directory = Path(record["directory"])
        resolved_source = source if source.is_absolute() else directory / source
        if not resolved_source.is_file():
            continue
        key = (str(resolved_source.resolve()), tuple(arguments))
        records[key] = {**record, "arguments": arguments}
    return [records[key] for key in sorted(records)]


def llvm_arguments(arguments: list[str], output: Path) -> list[str]:
    """Convert one captured object compile into an unoptimized LLVM IR replay."""
    rewritten: list[str] = []
    index = 0
    while index < len(arguments):
        argument = arguments[index]
        if argument in {"-c", "-MMD", "-MD"}:
            index += 1
            continue
        if argument in {"-MF", "-MT", "-MQ", "-o"}:
            index += 2
            continue
        if re.fullmatch(r"-O(?:[0-9sz]|fast|g)?", argument):
            index += 1
            continue
        rewritten.append(argument)
        index += 1
    rewritten.extend(["-w", "-O0", "-S", "-emit-llvm", "-o", str(output)])
    return rewritten


def replay_record(
    item: tuple[int, dict[str, Any]], clang: Path, ir_dir: Path
) -> tuple[int, dict[str, set[str]], dict[str, int], set[str]]:
    """Replay, parse, and remove one LLVM module to bound temporary disk use."""
    index, record = item
    output = ir_dir / f"{index:05d}.ll"
    result = subprocess.run(
        [str(clang), *llvm_arguments(record["arguments"], output)],
        cwd=record["directory"],
        check=False,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    if result.returncode != 0:
        source = source_argument(record["arguments"])
        diagnostic = result.stderr.decode("utf-8", errors="replace")
        output.unlink(missing_ok=True)
        raise RuntimeError(f"LLVM replay failed for {source}:\n{diagnostic}")
    try:
        graph, indirect, definitions = parse_ir(f"tu-{index:05d}", output)
    finally:
        output.unlink(missing_ok=True)
    return index, graph, indirect, definitions


def llvm_symbol_from_match(match: re.Match[str], prefix: str = "") -> str:
    """Read one quoted or plain LLVM symbol from a regex match."""
    return normalize_symbol(
        match.group(f"{prefix}quoted"),
        match.group(f"{prefix}plain"),
    )


def parse_ir(
    module_id: str, path: Path
) -> tuple[dict[str, set[str]], dict[str, int], set[str]]:
    """Parse direct calls, indirect-call counts, and definitions from one IR module."""
    raw_functions: dict[str, dict[str, Any]] = {}
    current: str | None = None
    current_data: dict[str, Any] | None = None
    for line in path.read_text(errors="replace").splitlines():
        definition = DEFINE_RE.match(line)
        if definition is not None:
            symbol = llvm_symbol_from_match(definition)
            internal = bool(
                re.search(r"\b(?:internal|private)\b", definition.group("prefix"))
            )
            current = f"{module_id}::{symbol}" if internal else symbol
            current_data = {
                "node": current,
                "internal": internal,
                "calls": [],
                "indirect": 0,
            }
            raw_functions[symbol] = current_data
            continue
        if current is None:
            continue
        if line.startswith("}"):
            current = None
            current_data = None
            continue
        if " call " not in line and " invoke " not in line:
            continue
        assert current_data is not None
        call = CALL_RE.search(line)
        if call is None:
            current_data["indirect"] += 1
            continue
        target = call.group("target")
        if target.startswith("%"):
            current_data["indirect"] += 1
        else:
            current_data["calls"].append(
                normalize_symbol(call.group("quoted"), call.group("plain"))
            )

    internal_symbols = {
        symbol for symbol, data in raw_functions.items() if data["internal"]
    }
    graph: dict[str, set[str]] = {}
    indirect: dict[str, int] = {}
    definitions: set[str] = set()
    for symbol, data in raw_functions.items():
        node = str(data["node"])
        definitions.add(node)
        graph[node] = {
            f"{module_id}::{target}" if target in internal_symbols else target
            for target in data["calls"]
        }
        indirect[node] = int(data["indirect"])
    return graph, indirect, definitions


def discover_arginfo_headers(
    records: list[dict[str, Any]], php_src: Path, build_dir: Path
) -> list[Path]:
    """Find generated arginfo headers referenced by configured translation units."""
    headers: set[Path] = set()
    for record in records:
        source = source_argument(record["arguments"])
        assert source is not None
        directory = Path(record["directory"])
        source = (source if source.is_absolute() else directory / source).resolve()
        content = source.read_text(errors="replace")
        for include in INCLUDE_RE.findall(content):
            candidates = (
                source.parent / include,
                php_src / include,
                build_dir / include,
            )
            for candidate in candidates:
                if candidate.is_file():
                    headers.add(candidate.resolve())
                    break
    return sorted(headers)


def active_arginfo(
    header: Path, preprocessor: Path, php_src: Path, build_dir: Path
) -> str:
    """Evaluate build guards while preserving generated registration macros."""
    result = subprocess.run(
        [
            str(preprocessor),
            "-E",
            "-fdirectives-only",
            "-include",
            str(build_dir / "main" / "php_config.h"),
            "-I",
            str(php_src),
            "-I",
            str(build_dir),
            str(header),
        ],
        check=False,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    if result.returncode != 0:
        raise SystemExit(
            f"arginfo preprocessing failed for {header}:\n"
            + result.stderr.decode("utf-8", errors="replace")
        )
    return result.stdout.decode("utf-8", errors="replace")


def split_macro_arguments(content: str) -> list[str]:
    """Split one generated macro invocation without losing nested expressions."""
    arguments: list[str] = []
    current: list[str] = []
    depth = 0
    quoted = False
    escaped = False
    for character in content:
        if quoted:
            current.append(character)
            if escaped:
                escaped = False
            elif character == "\\":
                escaped = True
            elif character == '"':
                quoted = False
            continue
        if character == '"':
            quoted = True
            current.append(character)
        elif character == "(":
            depth += 1
            current.append(character)
        elif character == ")":
            depth -= 1
            current.append(character)
        elif character == "," and depth == 0:
            arguments.append("".join(current).strip())
            current = []
        else:
            current.append(character)
    arguments.append("".join(current).strip())
    return arguments


def c_name(value: str) -> str:
    """Decode a generated C string or return an identifier unchanged."""
    value = value.strip()
    if value.startswith('"') and value.endswith('"'):
        return json.loads(value)
    return value


def macro_on_line(line: str) -> tuple[str, list[str]] | None:
    """Parse one generated registration macro invocation from a line."""
    match = re.search(r"\b(ZEND_[A-Z_]+)\((.*)\)\s*,?\s*$", line.strip())
    if match is None:
        return None
    return match.group(1), split_macro_arguments(match.group(2))


def parse_public_entries(contents: list[str]) -> list[dict[str, Any]]:
    """Map active PHP function/method entries to their compiled C handlers."""
    entries: dict[tuple[str, str, str | None], dict[str, Any]] = {}
    for content in contents:
        current_class: str | None = None
        for line in content.splitlines():
            class_table = CLASS_TABLE_RE.search(line)
            if class_table is not None:
                current_class = class_table.group(1)
                continue
            if current_class is not None and line.strip() == "};":
                current_class = None
                continue
            parsed = macro_on_line(line)
            if parsed is None:
                continue
            macro, arguments = parsed
            entry: dict[str, Any] | None = None
            if macro == "ZEND_FE" and len(arguments) >= 1:
                name = c_name(arguments[0])
                entry = {"kind": "function", "name": name, "handler": f"zif_{name}"}
            elif macro == "ZEND_FALIAS" and len(arguments) >= 2:
                name = c_name(arguments[0])
                alias = c_name(arguments[1])
                entry = {
                    "kind": "function",
                    "name": name,
                    "handler": f"zif_{alias}",
                    "alias_of": alias,
                }
            elif macro == "ZEND_NAMED_FE" and len(arguments) >= 2:
                entry = {
                    "kind": "function",
                    "name": c_name(arguments[0]),
                    "handler": c_name(arguments[1]),
                }
            elif macro == "ZEND_RAW_FENTRY" and len(arguments) >= 2:
                handler = c_name(arguments[1])
                if handler != "NULL":
                    kind = "method" if current_class is not None else "function"
                    entry = {
                        "kind": kind,
                        "name": c_name(arguments[0]),
                        "handler": handler,
                    }
                    if current_class is not None:
                        entry["class"] = current_class
            elif macro == "ZEND_ME" and len(arguments) >= 2:
                class_name = c_name(arguments[0])
                name = c_name(arguments[1])
                entry = {
                    "kind": "method",
                    "class": class_name,
                    "name": name,
                    "handler": f"zim_{class_name}_{name}",
                }
            elif macro == "ZEND_MALIAS" and len(arguments) >= 3:
                class_name = c_name(arguments[0])
                name = c_name(arguments[1])
                alias = c_name(arguments[2])
                entry = {
                    "kind": "method",
                    "class": class_name,
                    "name": name,
                    "handler": f"zim_{class_name}_{alias}",
                    "alias_of": alias,
                }
            elif macro == "ZEND_NAMED_ME" and len(arguments) >= 2:
                entry = {
                    "kind": "method",
                    "class": current_class or "unknown",
                    "name": c_name(arguments[0]),
                    "handler": c_name(arguments[1]),
                }
            elif macro == "ZEND_ME_MAPPING" and len(arguments) >= 2:
                entry = {
                    "kind": "method",
                    "class": current_class or "unknown",
                    "name": c_name(arguments[0]),
                    "handler": f"zif_{c_name(arguments[1])}",
                }
            if entry is None:
                continue
            key = (entry["kind"], entry["name"], entry.get("class"))
            existing = entries.get(key)
            if existing is not None and existing != entry:
                raise SystemExit(f"conflicting public entry mapping: {existing} / {entry}")
            entries[key] = entry
    ordered = [
        entries[key]
        for key in sorted(
            entries,
            key=lambda value: (
                value[0],
                (value[2] or "").lower(),
                value[1].lower(),
            ),
        )
    ]
    infer_entry_aliases(ordered)
    return ordered


def infer_entry_aliases(entries: list[dict[str, Any]]) -> None:
    """Resolve raw generated aliases from their canonical compiled handler."""
    canonical_by_handler: dict[str, str] = {}
    for entry in entries:
        handler = entry["handler"]
        if entry["kind"] == "function" and handler == f"zif_{entry['name']}":
            canonical_by_handler[handler] = entry["name"]
        elif (
            entry["kind"] == "method"
            and handler == f"zim_{entry.get('class')}_{entry['name']}"
        ):
            canonical_by_handler[handler] = (
                f"{entry.get('class')}::{entry['name']}"
            )
    for entry in entries:
        if entry.get("alias_of") is not None:
            continue
        canonical = canonical_by_handler.get(entry["handler"])
        own_name = (
            entry["name"]
            if entry["kind"] == "function"
            else f"{entry.get('class')}::{entry['name']}"
        )
        if canonical is not None and canonical.lower() != own_name.lower():
            entry["alias_of"] = canonical


def reverse_reachable(graph: dict[str, set[str]]) -> tuple[set[str], set[str]]:
    """Return all nodes that directly or transitively reach a stream root."""
    nodes = set(graph)
    for callees in graph.values():
        nodes.update(callees)
    roots = {
        node
        for node in nodes
        if not node.split("::")[-1].startswith(("zif_", "zim_"))
        and ROOT_RE.search(node.split("::")[-1])
    }
    reverse: dict[str, set[str]] = defaultdict(set)
    for caller, callees in graph.items():
        for callee in callees:
            reverse[callee].add(caller)
    reachable = set(roots)
    queue = deque(sorted(roots))
    while queue:
        callee = queue.popleft()
        for caller in sorted(reverse.get(callee, ())):
            if caller not in reachable:
                reachable.add(caller)
                queue.append(caller)
    return roots, reachable


def shortest_root_path(
    start: str, graph: dict[str, set[str]], roots: set[str]
) -> list[str]:
    """Return one deterministic shortest direct-call path to a stream root."""
    queue: deque[tuple[str, list[str]]] = deque([(start, [start])])
    visited = {start}
    while queue:
        node, path = queue.popleft()
        if node in roots:
            return path
        for callee in sorted(graph.get(node, ())):
            if callee not in visited:
                visited.add(callee)
                queue.append((callee, [*path, callee]))
    return []


def classify_entries(
    entries: list[dict[str, Any]],
    graph: dict[str, set[str]],
    indirect: dict[str, int],
    roots: set[str],
    reachable: set[str],
) -> list[dict[str, Any]]:
    """Classify every public handler by statically resolved direct-call reachability."""
    classified: list[dict[str, Any]] = []
    for original in entries:
        entry = dict(original)
        handler = entry["handler"]
        if handler in reachable:
            entry["classification"] = "direct-stream-path"
            entry["path"] = shortest_root_path(handler, graph, roots)
        else:
            entry["classification"] = "no-stream-path"
            entry["handler_indirect_call_sites"] = indirect.get(handler, 0)
        classified.append(entry)
    return classified


def declared_class_names(contents: list[str]) -> set[str]:
    """Extract active PHP class names from generated registration functions."""
    names: set[str] = set()
    pattern = re.compile(r'INIT_CLASS_ENTRY\([^,]+,\s*"([^"]+)"')
    for content in contents:
        names.update(pattern.findall(content))
    return names


def reachability_surface(
    classified: list[dict[str, Any]], declared_classes: set[str]
) -> tuple[list[dict[str, Any]], list[dict[str, Any]]]:
    """Project direct stream entries into probe-compatible function/class lists."""
    functions: list[dict[str, Any]] = []
    classes: dict[str, list[dict[str, Any]]] = defaultdict(list)
    for entry in classified:
        if entry["classification"] != "direct-stream-path":
            continue
        if entry["kind"] == "function":
            functions.append(
                {
                    "name": entry["name"],
                    "alias_of": entry.get("alias_of"),
                    "handler": entry["handler"],
                    "path": entry["path"],
                }
            )
        else:
            classes[entry["class"]].append(
                {
                    "name": entry["name"],
                    "alias_of": entry.get("alias_of"),
                    "handler": entry["handler"],
                    "path": entry["path"],
                }
            )
    for name in declared_classes:
        if name.lower() in {"php_user_filter", "streambucket"}:
            classes.setdefault(name, [])
    functions.sort(key=lambda entry: entry["name"].lower())
    class_entries = [
        {
            "name": name,
            "methods": sorted(methods, key=lambda entry: entry["name"].lower()),
        }
        for name, methods in sorted(classes.items(), key=lambda item: item[0].lower())
    ]
    return functions, class_entries


def default_output(target: str, profile: str) -> Path:
    """Return the checked-in source-reachability artifact path."""
    return (
        ROOT
        / "tests"
        / "php_oracle"
        / "reachability"
        / "streams"
        / f"php-{PHP_RELEASE}"
        / target
        / f"{profile}.json"
    )


def build_manifest(args: argparse.Namespace) -> dict[str, Any]:
    """Replay the configured build and generate one reachability manifest."""
    php_src = args.php_src.resolve()
    build_dir = args.build_dir.resolve()
    compile_log = args.compile_log.resolve()
    if git_value(php_src, "HEAD") != PHP_SRC_COMMIT:
        raise SystemExit(f"php-src must be at frozen commit {PHP_SRC_COMMIT}")
    clang = args.clang.resolve() if args.clang else Path(shutil.which("clang") or "")
    if not clang.is_file():
        raise SystemExit("clang is required; pass --clang")
    directives_preprocessor = (
        args.directives_preprocessor.resolve()
        if args.directives_preprocessor
        else clang
    )
    if not directives_preprocessor.is_file():
        raise SystemExit(
            "directives preprocessor is required; pass --directives-preprocessor"
        )
    records = load_compile_records(compile_log)
    if not records:
        raise SystemExit("compiler capture contains no C translation units")
    headers = discover_arginfo_headers(records, php_src, build_dir)
    if not headers:
        raise SystemExit("configured build exposed no generated arginfo headers")

    with tempfile.TemporaryDirectory(prefix="elephc-stream-reachability-") as name:
        ir_dir = Path(name)
        with concurrent.futures.ThreadPoolExecutor(max_workers=args.jobs) as executor:
            futures = [
                executor.submit(replay_record, item, clang, ir_dir)
                for item in enumerate(records)
            ]
            modules = sorted(future.result() for future in futures)
        graph: dict[str, set[str]] = {}
        indirect: dict[str, int] = {}
        definitions: set[str] = set()
        for _, module_graph, module_indirect, module_definitions in modules:
            graph.update(module_graph)
            indirect.update(module_indirect)
            definitions.update(module_definitions)

    active_headers = [
        active_arginfo(
            header,
            directives_preprocessor,
            php_src,
            build_dir,
        )
        for header in headers
    ]
    entries = parse_public_entries(active_headers)
    roots, reachable = reverse_reachable(graph)
    classified = classify_entries(entries, graph, indirect, roots, reachable)
    functions, classes = reachability_surface(
        classified,
        declared_class_names(active_headers),
    )
    return {
        "schema_version": SCHEMA_VERSION,
        "kind": "php-src-stream-source-reachability",
        "gate": {
            "number": 0,
            "status": "candidate",
            "open_requirements": [],
        },
        "profile": {
            "php_release": PHP_RELEASE,
            "php_src_commit": PHP_SRC_COMMIT,
            "php_src_tree": git_value(php_src, "HEAD^{tree}"),
            "target": args.target,
            "build_profile": args.build_profile,
            "compile_capture_sha256": sha256_file(compile_log),
            "clang": str(clang),
            "clang_version": subprocess.run(
                [str(clang), "--version"],
                check=True,
                stdout=subprocess.PIPE,
                text=True,
            ).stdout.splitlines()[0],
            "directives_preprocessor": str(directives_preprocessor),
            "directives_preprocessor_version": subprocess.run(
                [str(directives_preprocessor), "--version"],
                check=True,
                stdout=subprocess.PIPE,
                text=True,
            ).stdout.splitlines()[0],
        },
        "analysis": {
            "translation_units": len(records),
            "arginfo_headers": [
                (
                    header.relative_to(php_src).as_posix()
                    if php_src in header.parents
                    else header.relative_to(build_dir).as_posix()
                )
                for header in headers
            ],
            "definitions": len(definitions),
            "direct_call_edges": sum(len(callees) for callees in graph.values()),
            "indirect_call_sites": sum(indirect.values()),
            "indirect_call_policy": (
                "Opaque C function-pointer callbacks do not establish source "
                "reachability unless a statically resolved direct path from the "
                "public handler reaches a stream boundary."
            ),
            "stream_roots": sorted(roots),
            "public_entries": classified,
            "summary": {
                "public_entries": len(classified),
                "direct_stream_entries": sum(
                    entry["classification"] == "direct-stream-path"
                    for entry in classified
                ),
                "no_stream_entries": sum(
                    entry["classification"] == "no-stream-path"
                    for entry in classified
                ),
                "unresolved_indirect_entries": 0,
            },
        },
        "functions": functions,
        "classes": classes,
        "generator": {
            "script": Path(__file__).relative_to(ROOT).as_posix(),
            "script_sha256": sha256_file(Path(__file__)),
        },
    }


def atomic_write(path: Path, content: bytes) -> None:
    """Replace one checked-in artifact atomically."""
    path.parent.mkdir(parents=True, exist_ok=True)
    descriptor, temporary = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    try:
        with os.fdopen(descriptor, "wb") as handle:
            handle.write(content)
        os.replace(temporary, path)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)


def main() -> int:
    """Generate or byte-check one configured-build reachability manifest."""
    args = parse_args()
    manifest = build_manifest(args)
    content = canonical_bytes(manifest)
    output = args.output or default_output(args.target, args.build_profile)
    if args.check:
        if not output.exists():
            print(f"missing reachability manifest: {output}", file=sys.stderr)
            return 1
        if output.read_bytes() != content:
            print(f"reachability drift: regenerate {output}", file=sys.stderr)
            return 1
        return 0
    atomic_write(output, content)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
