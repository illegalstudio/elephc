"""Regression tests for generic native-trace/static-symbol execution classification."""

from __future__ import annotations

import importlib.util
import json
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).parents[1] / "classify_execution_ledger.py"
SPEC = importlib.util.spec_from_file_location("classify_execution_ledger", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
MODULE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = MODULE
SPEC.loader.exec_module(MODULE)


class ClassifyExecutionLedgerTest(unittest.TestCase):
    """Check that generic trace kinds join to contracts and emitted labels correctly."""

    def test_classifies_builtin_contracts_and_retained_user_symbols(self) -> None:
        """Keep AOT/dynamic contract state distinct from static label retention."""

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            symbols = root / "symbols.tsv"
            symbols.write_text(
                "count\tkind\tsymbol\n"
                "2\tbuiltin\ttrim\n"
                "1\tbuiltin\tmissing\n"
                "1\tbuiltin\tweb_only\n"
                "3\tinstance_method\tDemo\\Box->Pass\n"
                "1\tfunction\tdemo_function\n"
                "1\treflection\tReflectionClass->getName\n"
                "1\tinclude\tinclude\n",
                encoding="utf-8",
            )
            edges = root / "edges.tsv"
            edges.write_text(
                "count\tcaller\tcallee\tkind\tsource_file\tsource_line\n"
                "2\tmain\ttrim\tbuiltin\t/app.php\t1\n"
                "3\tmain\tDemo\\Box->Pass\tinstance_method\t/app.php\t2\n",
                encoding="utf-8",
            )
            assembly = root / "program.s"
            assembly.write_text(
                "_method___Demo_N_Box___pass:\n"
                "_fn_demo_u_function:\n"
                "_method___Demo_N_Box___pass_epilogue:\n",
                encoding="utf-8",
            )
            registry = root / "builtin_registry.json"
            registry.write_text(
                json.dumps(
                    [
                        {
                            "canonical_name": "trim",
                            "aot": {"supported": True},
                            "eval": {"supported": True},
                        }
                    ]
                ),
                encoding="utf-8",
            )
            prelude = root / "prelude.php"
            prelude.write_text("function web_only(): void {}\n", encoding="utf-8")

            output = root / "classified"
            summary = MODULE.build_ledger(symbols, edges, assembly, registry, output, [prelude])

            self.assertEqual(1, summary["contract_aot_and_dynamic"])
            self.assertEqual(1, summary["missing_shared_contract"])
            self.assertEqual(1, summary["mode_prelude_aot_only"])
            self.assertEqual(2, summary["static_symbol_retained"])
            self.assertEqual(1, summary["reflection_runtime_provenance_required"])
            self.assertEqual(1, summary["resolver_provenance_required"])
            symbol_output = (output / "symbols.tsv").read_text(encoding="utf-8")
            self.assertIn("Demo\\Box->Pass\tstatic_symbol_retained", symbol_output)
            self.assertIn("demo_function\tstatic_symbol_retained", symbol_output)


if __name__ == "__main__":
    unittest.main()
