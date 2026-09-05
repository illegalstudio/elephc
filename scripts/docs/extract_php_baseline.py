#!/usr/bin/env python3
"""Snapshot the local PHP binary's internal functions, classes and constants into php_baseline.json.

Run manually when pinning or bumping the PHP baseline version (requires a local
PHP binary). CI never runs this: the snapshot is committed, so the comparison
generator works without PHP installed.

Usage:
    python3 scripts/docs/extract_php_baseline.py [--php /path/to/php]
"""
from __future__ import annotations

import argparse
import datetime
import json
import subprocess
import sys
from pathlib import Path

OUTPUT = Path(__file__).resolve().parent / "php_baseline.json"

# Extensions bundled with php-src 8.5 — its in-tree ext/ directory minus the
# Windows-only (com_dotnet) and test-harness (dl_test, skeleton, zend_test)
# entries — plus the two surfaces Reflection reports under other names:
# "core" (Zend) and "zend opcache". The snapshot keeps ONLY functions owned by
# these extensions, so a contributor's PECL / third-party modules can never
# contaminate the baseline, whatever they are named. Bundled extensions the
# local PHP does not load are recorded in the snapshot's "missing_bundled"
# field so an incomplete local build is visible in review instead of silently
# shrinking PHP's surface. PHP 8.5 added `uri` (RFC 3986 / WHATWG URL API) and
# `lexbor` (the HTML parser behind ext/uri and ext/dom).
BUNDLED_EXTENSIONS = frozenset({
    "core", "zend opcache", "uri", "lexbor",
    "bcmath", "bz2", "calendar", "ctype", "curl", "date", "dba", "dom",
    "enchant", "exif", "ffi", "fileinfo", "filter", "ftp", "gd", "gettext",
    "gmp", "hash", "iconv", "intl", "json", "ldap", "libxml", "mbstring",
    "mysqli", "mysqlnd", "odbc", "openssl", "pcntl", "pcre", "pdo",
    "pdo_dblib", "pdo_firebird", "pdo_mysql", "pdo_odbc", "pdo_pgsql",
    "pdo_sqlite", "pgsql", "phar", "posix", "random", "readline",
    "reflection", "session", "shmop", "simplexml", "snmp", "soap", "sockets",
    "sodium", "spl", "sqlite3", "standard", "sysvmsg", "sysvsem", "sysvshm",
    "tidy", "tokenizer", "xml", "xmlreader", "xmlwriter", "xsl", "zip",
    "zlib",
})

PHP_PROGRAM = r"""
$functions = [];
foreach (get_defined_functions()["internal"] as $f) {
    $r = new ReflectionFunction($f);
    $ext = $r->getExtensionName();
    $functions[strtolower($f)] = $ext === false ? "core" : strtolower($ext);
}
ksort($functions);
// Classes and constants are attributed to their owning extension by
// ReflectionExtension, the same authority PHP uses for ReflectionFunction.
$classes = [];
$constants = [];
$encode = function ($value) {
    if (is_float($value)) {
        if (is_nan($value)) return ["float" => "NAN"];
        if (is_infinite($value)) return ["float" => $value > 0 ? "INF" : "-INF"];
        return $value;
    }
    if (is_resource($value)) return ["resource" => get_resource_type($value)];
    if (is_object($value)) return ["object" => get_class($value)];
    return $value;
};
foreach (get_loaded_extensions() as $extName) {
    $ext = strtolower($extName);
    $r = new ReflectionExtension($extName);
    foreach ($r->getClasses() as $c) {
        $kind = $c->isInterface() ? "interface" : ($c->isEnum() ? "enum" : ($c->isTrait() ? "trait" : "class"));
        $classes[strtolower($c->getName())] = ["name" => $c->getName(), "kind" => $kind, "extension" => $ext];
    }
    foreach ($r->getConstants() as $name => $value) {
        $constants[$name] = ["extension" => $ext, "value" => $encode($value)];
    }
}
ksort($classes);
ksort($constants);
$exts = array_map("strtolower", get_loaded_extensions());
sort($exts);
echo json_encode([
    "php_version" => PHP_VERSION,
    "extensions" => $exts,
    "functions" => $functions,
    "classes" => $classes,
    "constants" => $constants,
], JSON_UNESCAPED_SLASHES | JSON_PRESERVE_ZERO_FRACTION);
"""


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--php", default="php", help="PHP binary to snapshot (default: php)")
    args = parser.parse_args()

    try:
        proc = subprocess.run(
            [args.php, "-r", PHP_PROGRAM], capture_output=True, text=True, check=False
        )
    except FileNotFoundError:
        print(f"error: PHP binary '{args.php}' not found; install PHP or pass --php", file=sys.stderr)
        return 1
    if proc.returncode != 0:
        print(proc.stderr, file=sys.stderr)
        print(f"error: '{args.php} -r' exited with {proc.returncode}", file=sys.stderr)
        return 1

    raw = json.loads(proc.stdout)

    original_funcs = len(raw["functions"])
    loaded = raw["extensions"]
    dropped_exts = sorted(set(loaded) - BUNDLED_EXTENSIONS)
    missing_bundled = sorted(BUNDLED_EXTENSIONS - set(loaded))
    kept_exts = [e for e in loaded if e in BUNDLED_EXTENSIONS]
    kept_funcs = {
        name: ext for name, ext in raw["functions"].items()
        if ext in BUNDLED_EXTENSIONS
    }
    n_dropped = original_funcs - len(kept_funcs)
    kept_classes = {
        key: entry for key, entry in raw["classes"].items()
        if entry["extension"] in BUNDLED_EXTENSIONS
    }
    kept_constants = {
        name: entry for name, entry in raw["constants"].items()
        if entry["extension"] in BUNDLED_EXTENSIONS
    }

    data = {
        "php_version": raw["php_version"],
        "generated_at": datetime.date.today().isoformat(),
        "extensions": kept_exts,
        "missing_bundled": missing_bundled,
        "functions": kept_funcs,
        "classes": kept_classes,
        "constants": kept_constants,
    }
    OUTPUT.write_text(json.dumps(data, indent=1, ensure_ascii=False) + "\n", encoding="utf-8")
    print(
        f"wrote {OUTPUT} (PHP {data['php_version']}, {len(kept_funcs)} functions, "
        f"{len(kept_classes)} classes, {len(kept_constants)} constants; "
        f"dropped {n_dropped} functions from {len(dropped_exts)} non-bundled extensions"
        + (f": {', '.join(dropped_exts)}" if dropped_exts else "")
        + ")"
    )
    if missing_bundled:
        print(
            f"warning: {len(missing_bundled)} bundled extensions are not loaded by this PHP "
            f"and are absent from the snapshot: {', '.join(missing_bundled)}",
            file=sys.stderr,
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
