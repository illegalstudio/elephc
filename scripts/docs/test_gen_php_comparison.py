"""Unit tests for gen_php_comparison.py. Run from the repo root:

    python3 -m unittest discover -s scripts/docs -p "test_*.py" -v
"""
from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path

import gen_php_comparison as gen


def public(name, area="String", eval_supported=True, eval_only=False,
           is_internal=False, is_extension=False, description="Does a thing.",
           target_support=None):
    entry = {
        "name": name, "area": area, "description": description,
        "is_internal": is_internal, "is_extension": is_extension,
        "eval_only": eval_only, "eval": {"supported": eval_supported},
    }
    if target_support is not None:
        entry["semantics"] = {"target_support": target_support}
    return entry


BASELINE = {
    "php_version": "8.4.20",
    "generated_at": "2026-08-11",
    "target": "macos-aarch64",
    "extensions": ["pcre", "standard"],
    "functions": {
        "preg_match": "pcre", "sprintf": "standard",
        "strlen": "standard", "strrev": "standard",
    },
}

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
    def __init__(self, tmp, registry, baseline=BASELINE, catalog=CATALOG_OK):
        self.root = Path(tmp)
        sd = self.root / "scripts" / "docs"
        sd.mkdir(parents=True)
        (self.root / "docs" / "php").mkdir(parents=True)
        tests = self.root / "tests" / "codegen"
        tests.mkdir(parents=True)
        (tests / "generators.rs").write_text("fn test_gen_basic() {}\n")
        (sd / "builtin_registry.json").write_text(json.dumps(registry))
        (sd / "php_baseline.json").write_text(json.dumps(baseline))
        (sd / "comparison_catalog.toml").write_text(catalog)

    @property
    def output(self):
        return self.root / "docs" / "php" / "compatibility.md"


class ValidationTests(unittest.TestCase):
    def run_gen(self, **kwargs):
        with tempfile.TemporaryDirectory() as tmp:
            fx = Fixture(tmp, **kwargs)
            code = gen.run(fx.root)
            out = fx.output.read_text() if fx.output.exists() else None
            return code, out

    def test_happy_path_writes_page(self):
        code, out = self.run_gen(registry=[public("strlen")])
        self.assertEqual(code, 0)
        self.assertIn("compatibility", out.lower())

    def test_supported_without_evidence_fails(self):
        catalog = '[[language]]\nfeature = "X"\nstatus = "supported"\n'
        code, out = self.run_gen(registry=[public("strlen")], catalog=catalog)
        self.assertEqual(code, 1)
        self.assertIsNone(out)

    def test_dangling_evidence_fails(self):
        catalog = ('[[language]]\nfeature = "X"\nstatus = "supported"\n'
                   'evidence = "tests/codegen/nope.rs"\n')
        code, _ = self.run_gen(registry=[public("strlen")], catalog=catalog)
        self.assertEqual(code, 1)

    def test_evidence_as_test_fn_name_passes(self):
        catalog = ('[[language]]\nfeature = "X"\nstatus = "supported"\n'
                   'evidence = "test_gen_basic"\n')
        code, _ = self.run_gen(registry=[public("strlen")], catalog=catalog)
        self.assertEqual(code, 0)

    def test_unknown_status_fails(self):
        catalog = '[[language]]\nfeature = "X"\nstatus = "wip"\n'
        code, _ = self.run_gen(registry=[public("strlen")], catalog=catalog)
        self.assertEqual(code, 1)

    def test_public_builtin_missing_from_baseline_fails(self):
        code, _ = self.run_gen(registry=[public("frobnicate")])
        self.assertEqual(code, 1)

    def test_php_builtin_for_another_target_is_disclosed_but_not_counted(self):
        linux_only = public("pcntl_getcpu", target_support=["linux-aarch64", "linux-x86_64"])
        code, out = self.run_gen(registry=[public("strlen"), linux_only])
        self.assertEqual(code, 0)
        self.assertIn("target-specific PHP functions", out)
        self.assertIn("`pcntl_getcpu()`", out)
        self.assertNotIn("pcntl_getcpu() |", out)

    def test_language_construct_names_are_tolerated(self):
        self.assertIn("isset", gen.LANGUAGE_CONSTRUCTS)
        code, _ = self.run_gen(registry=[public("strlen"), public("isset")])
        self.assertEqual(code, 0)

    def test_not_in_baseline_builtin_renders_in_beyond_php(self):
        code, out = self.run_gen(registry=[public("strlen"), public("clamp")])
        self.assertEqual(code, 0)
        self.assertIn("`clamp()`", out)            # Beyond PHP section
        self.assertIn("No PHP equivalent", out)    # its NOT_IN_BASELINE note

    def test_internal_and_extension_builtins_are_skipped(self):
        reg = [public("strlen"),
               public("__elephc_ptr", is_internal=True),
               public("lfc_special", is_extension=True)]
        code, out = self.run_gen(registry=reg)
        self.assertEqual(code, 0)
        self.assertIn("lfc_special", out)          # Beyond PHP section
        self.assertNotIn("__elephc_ptr", out)      # internals never rendered

    def test_functionless_extensions_are_mentioned(self):
        baseline = dict(BASELINE, extensions=[*BASELINE["extensions"], "reflection"])
        code, out = self.run_gen(registry=[public("strlen")], baseline=baseline)
        self.assertEqual(code, 0)
        self.assertIn("no procedural functions", out)
        self.assertIn("`reflection`", out)

    def test_missing_bundled_extensions_are_disclosed(self):
        baseline = dict(BASELINE, missing_bundled=["snmp", "tidy"])
        code, out = self.run_gen(registry=[public("strlen")], baseline=baseline)
        self.assertEqual(code, 0)
        self.assertIn("were not loaded", out)
        self.assertIn("`snmp`", out)
        self.assertIn("`tidy`", out)

    def test_module_marker_from_catalog_modules_key(self):
        catalog = (CATALOG_OK
                   + '\n[[extensions]]\nfeature = "PCRE glue"\nstatus = "supported"\n'
                     'evidence = "tests/codegen/generators.rs"\nmodules = ["pcre"]\n')
        code, out = self.run_gen(registry=[public("strlen")], catalog=catalog)
        self.assertEqual(code, 0)
        self.assertIn("`pcre`†", out)
        self.assertIn("compiler rewrites or runtime preludes", out)

    def test_unknown_module_in_modules_key_fails(self):
        catalog = (CATALOG_OK
                   + '\n[[extensions]]\nfeature = "Bogus"\nstatus = "supported"\n'
                     'evidence = "tests/codegen/generators.rs"\nmodules = ["nope"]\n')
        code, out = self.run_gen(registry=[public("strlen")], catalog=catalog)
        self.assertEqual(code, 1)
        self.assertIsNone(out)


class RenderTests(unittest.TestCase):
    def test_counts_and_determinism(self):
        reg = [public("strlen"), public("strrev", eval_supported=False)]
        with tempfile.TemporaryDirectory() as tmp:
            fx = Fixture(tmp, registry=reg)
            self.assertEqual(gen.run(fx.root), 0)
            first = fx.output.read_text()
            self.assertEqual(gen.run(fx.root), 0)
            self.assertEqual(first, fx.output.read_text())  # byte-identical
        # standard: 2 supported of 3 (sprintf unsupported); pcre: 0 of 1
        self.assertIn("2 / 3", first)
        self.assertIn("0 / 1", first)
        # overall: 2 of 4 baseline functions
        self.assertIn("2 / 4", first)


if __name__ == "__main__":
    unittest.main()
