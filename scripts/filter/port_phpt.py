#!/usr/bin/env python3
"""Port php-src ext/filter PHPT fixtures into elephc codegen cases.

Called from:
- Repository maintenance: `python3 scripts/filter/port_phpt.py`.

Key details:
- Reads the vendored php-src 8.5.10 ext/filter tests directory.
- Materialises request superglobals and filter.default from the PHPT sections so CLI
  fixtures exercise the same filter semantics as the web SAPI originals.
- Emits tests/fixtures/filter_phpt/cases.json plus one Rust test entry point per PHPT.
"""

from __future__ import annotations

import json
import re
from pathlib import Path
from urllib.parse import unquote_plus

ROOT = Path(__file__).resolve().parents[2]
PHP_SRC = Path(
    "/Users/guillaumeloulier/PhpstormProjects/oss/php-src-8.5.10-oracle/ext/filter/tests"
)
FIXTURE_DIR = ROOT / "tests" / "fixtures" / "filter_phpt"
TEST_FILE = ROOT / "tests" / "codegen" / "filter_phpt.rs"

SECTION = re.compile(r"^--([A-Z_]+)--\s*$", re.M)

FILTER_IDS = {
    "int": 257,
    "boolean": 258,
    "float": 259,
    "validate_regexp": 272,
    "validate_domain": 277,
    "validate_url": 273,
    "validate_email": 274,
    "validate_ip": 275,
    "validate_mac": 276,
    "string": 513,
    "stripped": 513,
    "encoded": 514,
    "special_chars": 515,
    "full_special_chars": 522,
    "unsafe_raw": 516,
    "email": 517,
    "url": 518,
    "number_int": 519,
    "number_float": 520,
    "add_slashes": 523,
    "callback": 1024,
}


def sections(text: str) -> dict[str, str]:
    """Splits one PHPT file into its `--NAME--` sections."""
    matches = list(SECTION.finditer(text))
    result: dict[str, str] = {}
    for index, match in enumerate(matches):
        start = match.end()
        end = matches[index + 1].start() if index + 1 < len(matches) else len(text)
        result[match.group(1)] = text[start:end].strip("\n")
    return result


def php_single(value: str) -> str:
    """Renders a PHP single-quoted literal."""
    return "'" + value.replace("\\", "\\\\").replace("'", "\\'") + "'"


def parse_query(query: str) -> dict:
    """Parses an application/x-www-form-urlencoded payload, including PHP bracket keys."""
    root: dict = {}
    for part in query.split("&"):
        if not part:
            continue
        raw_name, _, raw_value = part.partition("=")
        name = unquote_plus(raw_name)
        value = unquote_plus(raw_value)
        path = re.match(r"^([^\[]*)((?:\[[^\]]*\])*)$", name)
        if not path:
            continue
        keys = [path.group(1)] + re.findall(r"\[([^\]]*)\]", path.group(2))
        cursor = root
        for index, key in enumerate(keys):
            last = index == len(keys) - 1
            if last:
                cursor[key] = value
            else:
                cursor = cursor.setdefault(key, {})
    return root

def php_array(value) -> str:
    """Renders a nested Python dict/list as a PHP array literal."""
    if isinstance(value, dict):
        entries = []
        for key, item in value.items():
            entries.append(f"{php_single(str(key))} => {php_array(item)}")
        return "[" + ", ".join(entries) + "]"
    if isinstance(value, list):
        return "[" + ", ".join(php_array(item) for item in value) + "]"
    return php_single(str(value))

def setup_code(path_name: str, section: dict[str, str]) -> str:
    """Builds the request-superglobal and filter.default setup prefix."""
    lines: list[str] = []
    ini = section.get("INI", "")
    for line in ini.splitlines():
        if line.strip().startswith("error_reporting="):
            expression = line.split("=", 1)[1].strip()
            lines.append(f"error_reporting({expression});")
    get = section.get("GET")
    post = section.get("POST")
    cookie = section.get("COOKIE")
    env = section.get("ENV")
    get_values = parse_query(get) if get is not None else None
    post_values = parse_query(post) if post is not None else None
    cookie_values: dict[str, str] = {}
    if cookie is not None:
        for pair in cookie.split(";"):
            pair = pair.strip()
            if not pair or "=" not in pair:
                continue
            name, value = pair.split("=", 1)
            name = name.strip()
            if name not in cookie_values:
                cookie_values[name] = unquote_plus(value)
    env_values: dict[str, str] = {}
    if env is not None:
        for line in env.splitlines():
            if "=" not in line:
                continue
            name, value = line.split("=", 1)
            env_values[name] = value

    default = None
    flags = "0"
    for line in ini.splitlines():
        if line.strip().startswith("filter.default="):
            default = line.split("=", 1)[1].strip()
        if line.strip().startswith("filter.default_flags="):
            raw = line.split("=", 1)[1].strip()
            flags = raw if raw else "0"

    if default is not None:
        deprecate = (
            default != "unsafe_raw"
            and "~E_DEPRECATED" not in ini
            and "display_errors=0" not in ini
        )
        seed = lambda constant, values: lines.append(
            f"__elephc_filter_seed_request({constant}, {php_array(values)}, "
            f"{php_single(default)}, {flags}, {str(deprecate).lower()});"
        )
        if get_values is not None:
            seed("INPUT_GET", get_values)
        if post_values is not None:
            seed("INPUT_POST", post_values)
        if cookie is not None:
            seed("INPUT_COOKIE", cookie_values)
        if env_values:
            seed("INPUT_ENV", env_values)
            seed("INPUT_SERVER", env_values)
            for name, value in env_values.items():
                lines.append(f"putenv({php_single(name + '=' + value)});")
        if get_values is None and post_values is None and cookie is None and not env_values:
            seed("INPUT_GET", {})
        return "\n".join(lines)

    if get_values is not None:
        lines.append(f"$_GET = {php_array(get_values)};")
    if post_values is not None:
        lines.append(f"$_POST = {php_array(post_values)};")
    if cookie is not None:
        lines.append(f"$_COOKIE = {php_array(cookie_values)};")
    if env_values:
        for name, value in env_values.items():
            lines.append(f"$_ENV[{php_single(name)}] = {php_single(value)};")
            lines.append(f"$_SERVER[{php_single(name)}] = {php_single(value)};")
            lines.append(f"putenv({php_single(name + '=' + value)});")
    if get_values is not None or post_values is not None:
        combined = dict(get_values or {})
        if post_values is not None:
            combined.update(post_values)
        lines.append(f"$_REQUEST = {php_array(combined)};")
    return "\n".join(lines)

def source_for(path_name: str, section: dict[str, str]) -> str:
    """Returns the executable PHP source with request setup prepended."""
    file = section.get("FILE") or section.get("FILEEOF")
    if file is None:
        raise ValueError("PHPT has no FILE/FILEEOF section")
    if "include __DIR__ . '/033_run.inc';" in file:
        inc = (PHP_SRC / "033_run.inc").read_text()
        inc = re.sub(r"\?>\s*$", "", inc.removeprefix("<?php"))
        file = file.replace("include __DIR__ . '/033_run.inc';", inc)
    file = file.replace("= &$", "=& $")

    assert file.startswith("<?php")
    body = file[len("<?php") :]
    body = re.sub(r"\?>\s*$", "", body)
    body = re.sub(r"^\s*ini_set\('html_errors'.*\)\s*;?\s*$", "", body, flags=re.M)
    setup = setup_code(path_name, section)
    return "<?php\n" + (setup + "\n" if setup else "") + body


def expected_for(name: str, section: dict[str, str]) -> tuple[str, bool]:
    """Returns (expected text, is-EXPECTF)."""
    if "EXPECT" in section:
        return section["EXPECT"], False
    if "EXPECTF" in section:
        return section["EXPECTF"], True
    if "EXPECTREGEX" in section:
        raise ValueError(f"{name}: EXPECTREGEX is not supported by this port")
    raise ValueError(f"{name}: PHPT has no expectation section")


def test_name(name: str) -> str:
    """Turns one PHPT filename into a Rust test identifier."""
    stem = name.removesuffix(".phpt").replace("-", "_").replace(".", "_").lower()
    return "test_phpt_" + re.sub(r"[^a-z0-9_]", "_", stem)


def main() -> None:
    """Generates the JSON fixture and the Rust data-driven test entry points."""
    cases = []
    for path in sorted(PHP_SRC.glob("*.phpt")):
        section = sections(path.read_text(errors="replace"))
        source = source_for(path.name, section)
        expected, expectf = expected_for(path.name, section)
        # PHPT --XFAIL-- cases keep their original EXPECT and record the output php-src
        # actually produces. The harness validates the known-failing behavior explicitly
        # instead of rewriting the expectation.
        xfail = "XFAIL" in section
        xfail_output = None
        if path.name == "bug67167.02.phpt":
            xfail_output = "bool(false)"
        elif path.name == "bug42718.phpt":
            xfail_output = expected + "\\000"
        elif path.name == "bug49184.phpt":
            xfail_output = expected

        if path.name == "filter_default_deprecation.phpt":
            expected = expected.replace(
                "in Unknown on line 0",
                "in %s on line %d",
            )
            expectf = True

        ini_pairs = []
        for line in section.get("INI", "").splitlines():
            if "=" in line:
                name, value = line.split("=", 1)
                ini_pairs.append([name.strip(), value.strip()])
        cases.append(
            {
                "name": path.name,
                "source": source,
                "expected": expected,
                "expectf": expectf,
                "xfail": xfail,
                "xfail_output": xfail_output,
                "ini": ini_pairs,
            }
        )

    FIXTURE_DIR.mkdir(parents=True, exist_ok=True)
    (FIXTURE_DIR / "cases.json").write_text(
        json.dumps(cases, ensure_ascii=False, indent=2) + "\n"
    )

    lines = [
        "//! Purpose:",
        "//! Ports every php-src 8.5.10 ext/filter PHPT fixture into the elephc codegen suite.",
        "//!",
        "//! Called from:",
        "//! - `cargo test` through Rust's test harness.",
        "//!",
        "//! Key details:",
        "//! - `cases.json` is generated by `scripts/filter/port_phpt.py`; each PHP source has its",
        "//!   `--GET--`/`--POST--`/`--COOKIE--`/`--ENV--` setup materialised so CLI runs match the",
        "//!   original web-SAPI expectations.",
        "//! - stdout is compared to expected output after diagnostics are split out; stderr is",
        "//!   matched against the expected Warning/Deprecated/Fatal lines with EXPECTF wildcards.",
        "",
        "use std::collections::HashMap;",
        "use std::sync::OnceLock;",
        "",
        "use crate::support::*;",
        "",
        "#[derive(serde::Deserialize)]",
        "struct Case {",
        "    name: String,",
        "    source: String,",
        "    expected: String,",
        "    expectf: bool,",
        "    /// `--XFAIL--` tests keep their original EXPECT and compare against the recorded",
        "    /// php-src failure output instead.",
        "    #[serde(default)]",
        "    xfail: bool,",
        "    #[serde(default)]",
        "    xfail_output: Option<String>,",
        "    ini: Vec<(String, String)>,",
        "}",
        "",
        "/// Returns the parsed fixture table, loaded once per test process.",
        "fn cases() -> &'static Vec<Case> {",
        "    static CASES: OnceLock<Vec<Case>> = OnceLock::new();",
        "    CASES.get_or_init(|| {",
        "        serde_json::from_str(include_str!(\"../fixtures/filter_phpt/cases.json\"))",
        "            .expect(\"filter PHPT fixture JSON must parse\")",
        "    })",
        "}",
        "",
        "/// Splits expected output into stdout lines and ordered diagnostic lines.",
        "fn split_diagnostics(expected: &str) -> (String, Vec<&str>) {",
        "    let mut stdout = String::new();",
        "    let mut diagnostics = Vec::new();",
        "    for line in expected.lines() {",
        "        if line.starts_with(\"Warning: \")",
        "            || line.starts_with(\"Deprecated: \")",
        "            || line.starts_with(\"Notice: \")",
        "            || line.starts_with(\"Fatal error: \")",
        "            || line.starts_with(\"Parse error: \")",
        "        {",
        "            diagnostics.push(line);",
        "            if stdout.ends_with(\"\\n\\n\") { stdout.pop(); }",
        "        } else {",
        "            if !stdout.is_empty() || !line.is_empty() {",
        "                stdout.push_str(line);",
        "            }",
        "            stdout.push('\\n');",
        "        }",
        "    }",
        "    (stdout, diagnostics)",
        "}",
        "",
        "/// Matches one EXPECTF pattern against text using PHP run-tests' placeholders.",
        "fn expectf_match(pattern: &str, text: &str) -> bool {",
        "    let mut tokens = Vec::new();",
        "    let mut chars = pattern.chars().peekable();",
        "    while let Some(ch) = chars.next() {",
        "        if ch == '%' {",
        "            if let Some(kind) = chars.next() {",
        "                if matches!(kind, 's' | 'd' | 'i' | 'x' | 'X' | 'o' | 'f' | 'e' | 'w' | 'a' | 'A' | 'c') {",
        "                    tokens.push(Token::Wildcard(kind));",
        "                } else {",
        "                    tokens.push(Token::Char('%'));",
        "                    tokens.push(Token::Char(kind));",
        "                }",
        "                continue;",
        "            }",
        "        }",
        "        tokens.push(Token::Char(ch));",
        "    }",
        "    let text: Vec<char> = text.chars().collect();",
        "    let mut memo = HashMap::new();",
        "    match_tokens(&tokens, &text, 0, 0, &mut memo)",
        "}",
        "",
        "#[derive(Clone, Copy)]",
        "enum Token { Char(char), Wildcard(char) }",
        "",
        "/// Returns whether one wildcard byte satisfies PHP's EXPECTF placeholder.",
        "fn wildcard_accepts(kind: char, ch: char) -> bool {",
        "    match kind {",
        "        'd' | 'i' => ch.is_ascii_digit(),",
        "        'x' | 'X' => ch.is_ascii_hexdigit(),",
        "        'o' => ('0'..='7').contains(&ch),",
        "        'B' => ch == '0' || ch == '1',",
        "        'w' => ch.is_whitespace(),",
        "        'f' | 'e' | 'E' => ch.is_ascii_digit() || matches!(ch, '.' | '+' | '-' | 'e' | 'E'),",
        "        'c' => true,",
        "        _ => !ch.is_whitespace() || kind == 's' || kind == 'a',",
        "    }",
        "}",
        "",
        "/// Recursive memoised EXPECTF matcher; wildcards try their longest valid run first.",
        "fn match_tokens(",
        "    tokens: &[Token],",
        "    text: &[char],",
        "    token_index: usize,",
        "    text_index: usize,",
        "    memo: &mut HashMap<(usize, usize), bool>,",
        ") -> bool {",
        "    if let Some(value) = memo.get(&(token_index, text_index)) {",
        "        return *value;",
        "    }",
        "    let result = match tokens.get(token_index) {",
        "        None => text_index == text.len(),",
        "        Some(Token::Char(expected)) => {",
        "            text.get(text_index) == Some(expected)",
        "                && match_tokens(tokens, text, token_index + 1, text_index + 1, memo)",
        "        }",
        "        Some(Token::Wildcard(kind)) => {",
        "            let any = *kind == 'A';",
        "            let mut end = text_index;",
        "            while end < text.len() && (any || text[end] != '\\n') && wildcard_accepts(*kind, text[end]) {",
        "                end += 1;",
        "            }",
        "            let mut matched = false;",
        "            loop {",
        "                if match_tokens(tokens, text, token_index + 1, end, memo) {",
        "                    matched = true;",
        "                    break;",
        "                }",
        "                if end == text_index { break; }",
        "                end -= 1;",
        "            }",
        "            matched",
        "        }",
        "    };",
        "    memo.insert((token_index, text_index), result);",
        "    result",
        "}",
        "",
        "/// Compares one fixture's captured output against the PHPT expectation.",
        "fn assert_case(case: &Case) {",
        "    let output = if case.ini.is_empty() {",
        "        compile_and_run_capture(&case.source)",
        "    } else {",
        "        compile_and_run_capture_with_ini(&case.source, &case.ini)",
        "    };",
        "    let expected_text = if case.xfail {",
        "        case.xfail_output.as_deref().unwrap_or(&case.expected)",
        "    } else {",
        "        &case.expected",
        "    };",
        "    let (expected_stdout, diagnostics) = split_diagnostics(expected_text);",
        "    let actual_stdout = output.stdout.trim_end_matches('\\n');",
        "    let expected_stdout = expected_stdout.trim_end_matches('\\n');",
        "    if case.expectf {",
        "        assert!(",
        "            expectf_match(expected_stdout, actual_stdout),",
        "            \"{}: stdout mismatch\\n--- expected ---\\n{:?}\\n--- actual ---\\n{:?}\",",
        "            case.name, expected_stdout, actual_stdout",
        "        );",
        "    } else {",
        "        assert_eq!(actual_stdout, expected_stdout, \"{}: stdout mismatch\", case.name);",
        "    }",
        "    if diagnostics.is_empty() {",
        "        assert!(output.stderr.is_empty(), \"{}: unexpected stderr: {}\", case.name, output.stderr);",
        "    } else {",
        "        let actual_lines: Vec<&str> = output.stderr.lines().collect();",
        "        assert_eq!(",
        "            actual_lines.len(), diagnostics.len(),",
        "            \"{}: diagnostic count mismatch\\n--- expected ---\\n{}\\n--- actual ---\\n{}\",",
        "            case.name, diagnostics.join(\"\\n\"), output.stderr",
        "        );",
        "        for (expected, actual) in diagnostics.iter().zip(actual_lines) {",
        "            let ok = if case.expectf { expectf_match(expected, actual) } else { expected == &actual };",
        "            assert!(ok, \"{}: diagnostic mismatch\\n--- expected ---\\n{}\\n--- actual ---\\n{}\", case.name, expected, actual);",
        "        }",
        "    }",
        "}",
        "",
        "/// Runs one fixture by index.",
        "fn run_case(index: usize) {",
        "    assert_case(&cases()[index]);",
        "}",
        "",
    ]
    for index, case in enumerate(cases):
        lines.extend(
            [
                f"/// Ports `{case['name']}`.",
                "#[test]",
                f"fn {test_name(case['name'])}() {{ run_case({index}); }}",
                "",
            ]
        )
    TEST_FILE.write_text("\n".join(lines).rstrip() + "\n")


if __name__ == "__main__":
    main()
