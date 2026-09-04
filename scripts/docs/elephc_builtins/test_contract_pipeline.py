"""Focused tests for shared-contract builtin documentation generation."""

from __future__ import annotations

import json
import sys
import tempfile
import unittest
from collections import Counter
from pathlib import Path
from unittest.mock import patch

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[2]
sys.path.insert(0, str(HERE))

import extract  # noqa: E402
import render  # noqa: E402

sys.path.insert(0, str(HERE.parent))
import audit_builtins  # noqa: E402


class ContractPipelineTests(unittest.TestCase):
    """Exercise exceptional support routes and presentation validation."""

    @classmethod
    def setUpClass(cls) -> None:
        """Load the prebuilt exporter once for all contract assertions."""
        cls.records = extract.run_gen_builtins(REPO)
        cls.by_name = {record["name"]: record for record in cls.records}
        registry = json.loads(
            (REPO / "scripts" / "docs" / "builtin_registry.json").read_text(
                encoding="utf-8"
            )
        )
        cls.render_by_name = {record["name"]: record for record in registry}

    def test_all_non_registry_contract_routes_are_exported(self) -> None:
        """Keep the six constructs, 58 preludes, and three eval-only routes explicit."""
        routes = Counter(
            (record.get("aot") or {}).get("kind")
            for record in self.records
            if (record.get("aot") or {}).get("kind") != "registry"
        )
        self.assertEqual(
            routes,
            Counter(
                {
                    "language-construct": 5,
                    "dedicated-syntax": 1,
                    # MEASURED on the merged catalogue: `dir()`, the 14 `gz*` stream
                    # functions, `zlib_get_coding_type` and `similar_text` from this
                    # branch, plus the four hash_* and the thirty-four PHP-visible
                    # curl_* contracts the canonical `--features curl` docs
                    # configuration publishes (see extract.run_gen_builtins).
                    "prelude": 58,
                    "none": 3,
                }
            ),
        )

    def test_hash_init_and_exit_use_backend_contract_signatures(self) -> None:
        """Pin the prelude subset and construct default that previously drifted."""
        hash_init = self.by_name["hash_init"]
        self.assertTrue(hash_init["aot"]["supported"])
        self.assertEqual(hash_init["aot"]["kind"], "prelude")
        self.assertEqual(hash_init["aot"]["signature_override_reason"], "prelude-signature-subset")
        self.assertEqual([param["name"] for param in hash_init["aot"]["params"]], ["algo"])
        self.assertEqual(self.by_name["exit"]["params"][0]["default"], 0)

    def test_getenv_user_signature_preserves_checked_union(self) -> None:
        """Render the checked `string|false` result instead of contract-level `mixed`."""
        self.assertEqual(
            self.render_by_name["getenv"]["sig"]["return_type"],
            "string|false",
        )

    def test_unknown_presentation_override_is_rejected(self) -> None:
        """Prevent dormant override keys from accumulating silently again."""
        with patch.dict(extract.AREA_BY_NAME, {"__unknown_contract": ("Misc", "Misc")}):
            with self.assertRaisesRegex(ValueError, "unknown contracts"):
                extract.validate_presentation_overrides(REPO, self.records)

    def test_prelude_availability_renders_both_effective_signatures(self) -> None:
        """Show the narrower AOT call and broader eval call without marking eval-only."""
        rendered = render._availability_section(self.render_by_name["hash_init"])
        self.assertIn("compiler-injected hash prelude", rendered)
        self.assertIn('hash_init(string $algo, int $flags = 0, string $key = "")', rendered)
        self.assertNotIn("Compiled (AOT)**: not available", rendered)

    def test_host_only_process_availability_keeps_eval_independent(self) -> None:
        """Label iOS AOT refusal without implying the Magician path is target-gated."""
        rendered = render._availability_section(self.render_by_name["system"])
        self.assertIn("three executable/release hosts", rendered)
        self.assertIn("refused at compile time for iOS library targets", rendered)
        self.assertIn("`eval()` (magician interpreter)**: supported", rendered)

    def test_prelude_availability_names_the_declaring_prelude(self) -> None:
        """Two preludes declare PHP-visible builtins; each page must name its own."""
        rendered = render._availability_section(self.render_by_name["curl_init"])
        self.assertIn("compiler-injected curl prelude", rendered)
        self.assertNotIn("hash prelude", rendered)
        self.assertNotIn("Compiled (AOT)**: not available", rendered)

    def test_user_renderer_owns_section_spacing_once(self) -> None:
        """Join empty optional sections without accumulating blank-line runs."""
        builtin = {
            **self.render_by_name["strlen"],
            "examples": [],
            "notes": [],
            "see_also": [],
        }
        rendered = render.render_user(builtin, 1, REPO)
        self.assertNotRegex(rendered, r"\n(?:[ \t]*\n){2,}")
        self.assertIn("usage patterns._\n\n## Internals", rendered)

    def test_docs_audit_rejects_excessive_blank_lines(self) -> None:
        """Keep the generated-tree audit sensitive to whitespace regressions."""
        with tempfile.TemporaryDirectory(dir=REPO) as tmp:
            page = Path(tmp) / "builtin.md"
            page.write_text("first\n\n\nsecond\n", encoding="utf-8")
            errors: list[str] = []
            audit_builtins._check_page_whitespace(page, errors)
        self.assertEqual(len(errors), 1)
        self.assertIn("excessive blank lines", errors[0])


if __name__ == "__main__":
    unittest.main()
