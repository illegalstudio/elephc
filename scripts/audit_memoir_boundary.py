#!/usr/bin/env python3
"""Audit Memoir's compiler-only Cargo and Rust source boundary without dependencies."""
from __future__ import annotations

import argparse
from dataclasses import dataclass
import json
from pathlib import Path
import re
import subprocess
import sys

MEMOIR = "elephc-memoir"
BANNED_TYPES = {"PhpType", "BuiltinId", "RuntimeFnId"}
BANNED_MODULES = {"codegen", "codegen_support", "types", "ir", "ir_lower", "pipeline"}
BANNED_ATTRIBUTES = {"link", "no_mangle", "export_name", "crate_type", "proc_macro"}


@dataclass(frozen=True)
class Finding:
    """One actionable boundary violation with a stable category."""
    code: str
    location: str
    message: str


def audit_metadata(metadata: dict, root: Path) -> list[Finding]:
    """Validate all workspace production edges, including aliases and target cfgs."""
    findings: list[Finding] = []
    root = root.resolve()
    if Path(metadata["workspace_root"]).resolve() != root:
        return [Finding("workspace", str(root), "Cargo metadata belongs to another workspace")]
    members = set(metadata["workspace_members"])
    packages = [p for p in metadata["packages"] if p["id"] in members]
    expected = root / "crates/elephc-memoir/Cargo.toml"
    matches = [p for p in packages if p["name"] == MEMOIR and Path(p["manifest_path"]).resolve() == expected]
    if len(matches) != 1:
        return [Finding("package", str(expected), "expected one local Memoir workspace package")]
    memoir = matches[0]
    loc = str(expected.relative_to(root))
    if memoir["dependencies"]:
        findings.append(Finding("dependency", loc, "Memoir must have no normal, dev, build or target-specific dependencies"))
    if memoir.get("links") is not None:
        findings.append(Finding("native-link", loc, "Memoir must not declare a native links library"))
    library = [t for t in memoir["targets"] if Path(t["src_path"]).resolve() == expected.parent / "src/lib.rs"]
    if (len(library) != 1 or library[0]["crate_types"] != ["rlib"]
            or any(set(t["crate_types"]) - {"rlib", "bin"} for t in memoir["targets"])):
        findings.append(Finding("crate-type", loc, "Memoir must produce only an ordinary rlib, never a native library"))
    if any(set(t["kind"]) & {"custom-build", "bin"} for t in memoir["targets"]):
        findings.append(Finding("native-build", loc, "Memoir must not have a build script or binary target"))
    compiler = [p for p in packages if Path(p["manifest_path"]).resolve() == root / "Cargo.toml"]
    if len(compiler) != 1:
        return findings + [Finding("compiler", "Cargo.toml", "expected one root compiler package")]
    compiler = compiler[0]
    by_path = {Path(p["manifest_path"]).resolve().parent: p["id"] for p in packages}
    by_name = {p["name"]: p["id"] for p in packages}
    graph: dict[str, list[str]] = {}
    direct_compiler = []
    for package in packages:
        edges = []
        for dependency in package["dependencies"]:
            if dependency.get("kind") is not None:
                continue  # Build/dev consumers are host tools, not generated-program dependencies.
            path = dependency.get("path")
            target = by_path.get(Path(path).resolve()) if path else by_name.get(dependency["name"])
            # Cargo's name is canonical even when rename changes the Rust dependency identifier.
            if dependency["name"] == MEMOIR:
                target = memoir["id"]
            if target:
                edges.append(target)
            if package["id"] == compiler["id"] and target == memoir["id"]:
                direct_compiler.append(dependency)
        graph[package["id"]] = edges
    if (len(direct_compiler) != 1 or direct_compiler[0].get("optional", False)
            or direct_compiler[0].get("target") is not None
            or Path(direct_compiler[0].get("path") or ".").resolve() != expected.parent):
        findings.append(Finding("compiler-link", "Cargo.toml", "compiler must depend unconditionally on the local Memoir rlib"))

    def reaches_memoir(start: str) -> bool:
        """Handle cycles without hiding indirect production consumers."""
        pending = list(graph.get(start, []))
        visited = set()
        while pending:
            item = pending.pop()
            if item == memoir["id"]:
                return True
            if item not in visited:
                visited.add(item)
                pending.extend(graph.get(item, []))
        return False

    for package in packages:
        if package["id"] not in {memoir["id"], compiler["id"]} and reaches_memoir(package["id"]):
            findings.append(Finding("runtime-consumer", package["name"], "production dependency reaches Memoir; only the compiler may consume it"))
    return findings


@dataclass(frozen=True)
class Token:
    """An identifier or punctuation token outside Rust comments and literals."""
    text: str
    line: int


def rust_tokens(source: str) -> list[Token]:
    """Skip nested comments, escaped/raw strings, chars and lifetime punctuation."""
    tokens = []
    i = 0
    line = 1
    while i < len(source):
        start = i
        if source[i].isspace():
            line += source[i] == "\n"
            i += 1
            continue
        if source.startswith("//", i):
            end = source.find("\n", i)
            i = len(source) if end < 0 else end
            continue
        if source.startswith("/*", i):
            depth = 1
            i += 2
            while depth and i < len(source):
                if source.startswith("/*", i):
                    depth += 1
                    i += 2
                elif source.startswith("*/", i):
                    depth -= 1
                    i += 2
                else:
                    i += 1
            if depth:
                raise ValueError("unterminated Rust block comment")
            line += source[start:i].count("\n")
            continue
        raw = re.match(r'(?:br|cr|r)(#*)"', source[i:])
        if raw:
            terminator = '"' + raw.group(1)
            end = source.find(terminator, i + raw.end())
            if end < 0:
                raise ValueError("unterminated Rust raw string")
            i = end + len(terminator)
            line += source[start:i].count("\n")
            continue
        prefix = 1 if source.startswith(('b"', 'c"'), i) else 0
        if source[i + prefix:i + prefix + 1] == '"':
            i += prefix + 1
            while i < len(source):
                if source[i] == "\\":
                    i += 2
                elif source[i] == '"':
                    i += 1
                    break
                else:
                    i += 1
            else:
                raise ValueError("unterminated Rust string")
            line += source[start:i].count("\n")
            continue
        char_start = i + 1 if source.startswith("b'", i) else i
        if source[char_start:char_start + 1] == "'":
            char_end = char_start + 2
            if source[char_start + 1:char_start + 2] == "\\":
                char_end = char_start + 3
                if source[char_start + 2:char_start + 4] == "u{":
                    close = source.find("}", char_start + 4)
                    char_end = close + 1 if close >= 0 else len(source)
                elif source[char_start + 2:char_start + 3] == "x":
                    char_end = char_start + 5
            if source[char_end:char_end + 1] == "'":
                i = char_end + 1
                continue
            # A lifetime is punctuation followed by an identifier, not a character literal.
        ident = re.match(r'(?:r#)?([A-Za-z_][A-Za-z0-9_]*)', source[i:])
        if ident:
            tokens.append(Token(ident.group(1), line))
            i += ident.end()
            continue
        if source.startswith("::", i):
            tokens.append(Token("::", line))
            i += 2
        else:
            tokens.append(Token(source[i], line))
            i += 1
    return tokens


def audit_source(location: str, source: str) -> list[Finding]:
    """Reject compiler semantic types and native-export attributes in real Rust code."""
    try:
        tokens = rust_tokens(source)
    except ValueError as error:
        return [Finding("rust-source", location, str(error))]
    findings = []
    for index, token in enumerate(tokens):
        where = f"{location}:{token.line}"
        if token.text in BANNED_TYPES:
            findings.append(Finding("php-eir", where, f"{token.text} belongs to compiler semantics"))
        if (token.text == "crate" and index + 2 < len(tokens)
                and tokens[index + 1].text == "::" and tokens[index + 2].text in BANNED_MODULES):
            findings.append(Finding("compiler-module", where, "compiler modules must not be copied into Memoir"))
        if token.text == "#":
            cursor = index + 1
            if cursor < len(tokens) and tokens[cursor].text == "!":
                cursor += 1
            if cursor < len(tokens) and tokens[cursor].text == "[":
                depth = 1
                cursor += 1
                while cursor < len(tokens) and depth:
                    item = tokens[cursor]
                    depth += (item.text == "[") - (item.text == "]")
                    if item.text in BANNED_ATTRIBUTES:
                        findings.append(Finding("native-export", where, f"attribute {item.text} violates compiler-only emission"))
                    cursor += 1
    if location.endswith("/src/lib.rs"):
        required = ["#", "!", "[", "forbid", "(", "unsafe_code", ")", "]"]
        depth = 0
        unconditional = False
        for index, token in enumerate(tokens):
            if depth == 0 and [t.text for t in tokens[index:index + len(required)]] == required:
                unconditional = True
            depth += (token.text == "{") - (token.text == "}")
        if not unconditional:
            findings.append(Finding("unsafe-policy", location, "crate root must retain unconditional forbid(unsafe_code)"))
    return findings


def main() -> int:
    """Audit this checkout with Cargo's authoritative interpretation of its manifests."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    args = parser.parse_args()
    root = args.root.resolve()
    try:
        result = subprocess.run(["cargo", "metadata", "--no-deps", "--locked", "--format-version", "1"],
                                cwd=root, check=True, capture_output=True, text=True)
        metadata = json.loads(result.stdout)
        findings = audit_metadata(metadata, root)
        for path in sorted((root / "crates/elephc-memoir/src").rglob("*.rs")):
            findings.extend(audit_source(path.relative_to(root).as_posix(), path.read_text()))
    except (OSError, ValueError, KeyError, subprocess.CalledProcessError) as error:
        print(f"Memoir boundary audit could not complete: {error}", file=sys.stderr)
        return 2
    for finding in findings:
        print(f"{finding.location}: [{finding.code}] {finding.message}", file=sys.stderr)
    if findings:
        return 1
    print("Memoir boundary: dependency-free compiler rlib; no workspace runtime consumers or native exports.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
