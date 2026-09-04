"""Unit tests for gen_php_comparison.py. Run from the repo root:

    python3 -m unittest discover -s scripts/docs -p "test_*.py" -v
"""
from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path

import gen_php_comparison as gen


def public(name, module="standard", since=None, area="String", eval_supported=True,
           eval_only=False, is_internal=False, is_extension=False, aot_kind="registry",
           description="Does a thing."):
    return {
        "name": name, "canonical_name": name, "area": area, "description": description,
        "module": module, "since": since,
        "is_internal": is_internal, "is_extension": is_extension,
        "eval_only": eval_only, "eval": {"supported": eval_supported},
        "aot": {"kind": aot_kind, "supported": not eval_only},
    }


def klass(name, module="standard", kind="class", since=None, aot=True, eval_supported=True,
          extension=False, internal=False):
    return {
        "name": name, "kind": kind, "module": module, "bundled": module not in ("cairo", "imagick"),
        "since": since, "aot": {"supported": aot, "kind": "checker-injected"},
        "eval": {"supported": eval_supported, "kind": "interpreter"},
        "extension": extension, "internal": internal, "php_manual": None,
    }


def const(name, module="standard", value=1, since=None, route="predefined", eval_supported=True,
          extension=False, internal=False):
    return {
        "name": name, "module": module, "bundled": True, "since": since, "value": value,
        "route": route, "aot": {"supported": True, "kind": "language-intrinsic"},
        "eval": {"supported": eval_supported, "kind": "interpreter"},
        "extension": extension, "internal": internal,
    }


BASELINE = {
    "php_version": "8.5.10",
    "generated_at": "2026-09-04",
    "extensions": ["pcre", "standard"],
    "missing_bundled": [],
    "functions": {
        "preg_match": "pcre", "sprintf": "standard",
        "strlen": "standard", "strrev": "standard",
    },
    "classes": {
        "php_user_filter": {"name": "php_user_filter", "kind": "class", "extension": "standard"},
        "__php_incomplete_class": {"name": "__PHP_Incomplete_Class", "kind": "class", "extension": "standard"},
    },
    "constants": {
        "SORT_ASC": {"extension": "standard", "value": 4},
        "SORT_DESC": {"extension": "standard", "value": 3},
        "PREG_SPLIT_NO_EMPTY": {"extension": "pcre", "value": 1},
    },
}

SYMBOLS_EMPTY = {"classes": [], "constants": []}

CATALOG_OK = """
[[language]]
feature  = "Generators"
status   = "supported"
evidence = "tests/codegen/generators.rs"

[[limitations]]
title = "Static subset"
notes = "AOT only."
"""


class Fixture:
    def __init__(self, tmp, registry, baseline=BASELINE, catalog=CATALOG_OK, symbols=SYMBOLS_EMPTY):
        self.root = Path(tmp)
        sd = self.root / "scripts" / "docs"
        sd.mkdir(parents=True)
        (self.root / "docs" / "php").mkdir(parents=True)
        tests = self.root / "tests" / "codegen"
        tests.mkdir(parents=True)
        (tests / "generators.rs").write_text("fn test_gen_basic() {}\n")
        (sd / "builtin_registry.json").write_text(json.dumps(registry))
        (sd / "symbol_registry.json").write_text(json.dumps(symbols))
        (sd / "php_baseline.json").write_text(json.dumps(baseline))
        (sd / "comparison_catalog.toml").write_text(catalog)

    @property
    def output(self):
        return self.root / "docs" / "php" / "compatibility.md"


def run_gen(**kwargs):
    with tempfile.TemporaryDirectory() as tmp:
        fx = Fixture(tmp, **kwargs)
        code = gen.run(fx.root)
        out = fx.output.read_text() if fx.output.exists() else None
        return code, out


class ValidationTests(unittest.TestCase):
    def test_happy_path_writes_page(self):
        code, out = run_gen(registry=[public("strlen")])
        self.assertEqual(code, 0)
        self.assertIn("compatibility", out.lower())

    def test_supported_without_evidence_fails(self):
        catalog = '[[language]]\nfeature = "X"\nstatus = "supported"\n'
        code, out = run_gen(registry=[public("strlen")], catalog=catalog)
        self.assertEqual(code, 1)
        self.assertIsNone(out)

    def test_dangling_evidence_fails(self):
        catalog = ('[[language]]\nfeature = "X"\nstatus = "supported"\n'
                   'evidence = "tests/codegen/nope.rs"\n')
        code, _ = run_gen(registry=[public("strlen")], catalog=catalog)
        self.assertEqual(code, 1)

    def test_evidence_as_test_fn_name_passes(self):
        catalog = ('[[language]]\nfeature = "X"\nstatus = "supported"\n'
                   'evidence = "test_gen_basic"\n')
        code, _ = run_gen(registry=[public("strlen")], catalog=catalog)
        self.assertEqual(code, 0)

    def test_unknown_status_fails(self):
        catalog = '[[language]]\nfeature = "X"\nstatus = "wip"\n'
        code, _ = run_gen(registry=[public("strlen")], catalog=catalog)
        self.assertEqual(code, 1)

    def test_public_builtin_missing_from_baseline_fails(self):
        code, _ = run_gen(registry=[public("frobnicate")])
        self.assertEqual(code, 1)

    def test_module_mismatch_with_baseline_fails(self):
        code, _ = run_gen(registry=[public("strlen", module="pcre")])
        self.assertEqual(code, 1)

    def test_language_construct_kinds_are_tolerated(self):
        reg = [public("strlen"), public("isset", module="core", aot_kind="language-construct")]
        code, out = run_gen(registry=reg)
        self.assertEqual(code, 0)
        self.assertIn("language constructs", out)
        self.assertIn("`isset()`", out)

    def test_elephc_only_builtin_renders_in_beyond_php(self):
        code, out = run_gen(registry=[public("strlen"), public("clamp", module="elephc")])
        self.assertEqual(code, 0)
        self.assertIn("`clamp()`", out)            # Beyond PHP section
        self.assertIn("No PHP equivalent", out)    # its BEYOND_PHP_NOTES note

    def test_internal_skipped_and_extension_in_beyond_php(self):
        reg = [public("strlen"),
               public("__elephc_ptr", module="elephc", is_internal=True),
               public("lfc_special", is_extension=True)]
        code, out = run_gen(registry=reg)
        self.assertEqual(code, 0)
        self.assertIn("lfc_special", out)          # Beyond PHP section
        self.assertNotIn("__elephc_ptr", out)      # internals never rendered

    def test_symbol_newer_than_baseline_is_reported_not_counted(self):
        reg = [public("strlen"), public("array_first", since="8.6")]
        code, out = run_gen(registry=reg)
        self.assertEqual(code, 0)
        self.assertIn("AFTER this baseline", out)
        self.assertIn("`array_first()` (PHP 8.6)", out)
        self.assertIn("1 / 3", out)                # standard functions: only strlen counted

    def test_pecl_symbols_are_reported_separately(self):
        symbols = {"classes": [klass("Imagick", module="imagick")], "constants": []}
        code, out = run_gen(registry=[public("strlen")], symbols=symbols)
        self.assertEqual(code, 0)
        self.assertIn("PECL", out)
        self.assertIn("`imagick` (1 classes)", out)

    def test_empty_extensions_are_mentioned(self):
        baseline = dict(BASELINE, extensions=[*BASELINE["extensions"], "mysqlnd"])
        code, out = run_gen(registry=[public("strlen")], baseline=baseline)
        self.assertEqual(code, 0)
        self.assertIn("expose no functions, classes, or constants", out)
        self.assertIn("`mysqlnd`", out)

    def test_missing_bundled_extensions_are_disclosed(self):
        baseline = dict(BASELINE, missing_bundled=["snmp", "tidy"])
        code, out = run_gen(registry=[public("strlen")], baseline=baseline)
        self.assertEqual(code, 0)
        self.assertIn("were not loaded", out)
        self.assertIn("`snmp`", out)
        self.assertIn("`tidy`", out)

    def test_unknown_module_in_modules_key_fails(self):
        catalog = (CATALOG_OK
                   + '\n[[extensions]]\nfeature = "Bogus"\nstatus = "supported"\n'
                     'evidence = "tests/codegen/generators.rs"\nmodules = ["nope"]\n')
        code, out = run_gen(registry=[public("strlen")], catalog=catalog)
        self.assertEqual(code, 1)
        self.assertIsNone(out)

    def test_constant_value_divergence_fails_unless_recorded(self):
        symbols = {"classes": [], "constants": [const("SORT_ASC", value=99)]}
        code, _ = run_gen(registry=[public("strlen")], symbols=symbols)
        self.assertEqual(code, 1)

    def test_dynamic_constants_are_reported_not_cross_checked(self):
        symbols = {"classes": [], "constants": [const("SID", module="session", value="", route="dynamic")]}
        code, out = run_gen(registry=[public("strlen")], symbols=symbols)
        self.assertEqual(code, 0)
        self.assertIn("at runtime", out)
        self.assertIn("`SID`", out)


class RenderTests(unittest.TestCase):
    def test_counts_and_determinism(self):
        reg = [public("strlen"), public("strrev", eval_supported=False)]
        symbols = {
            "classes": [klass("php_user_filter")],
            "constants": [const("SORT_ASC", value=4), const("PREG_SPLIT_NO_EMPTY", module="pcre", value=1)],
        }
        with tempfile.TemporaryDirectory() as tmp:
            fx = Fixture(tmp, registry=reg, symbols=symbols)
            self.assertEqual(gen.run(fx.root), 0)
            first = fx.output.read_text()
            self.assertEqual(gen.run(fx.root), 0)
            self.assertEqual(first, fx.output.read_text())  # byte-identical
        # standard: functions 2 of 3, classes 1 of 2, constants 1 of 2; pcre: functions 0 of 1,
        # no classes, constants 1 of 1.
        self.assertIn("| `standard` | 2 / 3 · 67% | 1 / 2 · 50% | 1 / 2 · 50% |", first)
        self.assertIn("| `pcre` | 0 / 1 · 0% | — | 1 / 1 · 100% |", first)
        self.assertIn("functions **2 / 4**", first)
        self.assertIn("classes **1 / 2**", first)
        self.assertIn("constants **2 / 3**", first)
        # strrev is compiled-only, so the standard functions row diverges between backends.
        self.assertIn("- `standard` functions: 2 / 1", first)


if __name__ == "__main__":
    unittest.main()
