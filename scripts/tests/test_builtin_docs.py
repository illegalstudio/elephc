#!/usr/bin/env python3
"""Regression tests for generated builtin signatures and documentation metadata."""

from __future__ import annotations

import sys
import unittest
from pathlib import Path


DOCS_PACKAGE = Path(__file__).resolve().parents[1] / "docs" / "elephc_builtins"
sys.path.insert(0, str(DOCS_PACKAGE))

import extract  # noqa: E402
import registry  # noqa: E402
import render  # noqa: E402


class BuiltinDocsTests(unittest.TestCase):
    """Protect PHP-visible contracts that the scalar registry cannot express alone."""

    def test_signature_renders_by_reference_parameters(self) -> None:
        """Renders the PHP reference marker between the type and variable sigil."""
        builtin = {
            "name": "proc_open",
            "sig": {
                "params": [
                    {
                        "name": "pipes",
                        "type": "array",
                        "by_ref": True,
                        "default": None,
                        "optional": False,
                    }
                ],
                "variadic": None,
                "return_type": "resource|false",
            },
        }

        self.assertEqual(
            render._signature_line(builtin),
            "function proc_open(array &$pipes): resource|false",
        )

    def test_proc_open_overrides_preserve_php_types(self) -> None:
        """Keeps the command union, nullable options, and resource failure union."""
        self.assertEqual(
            registry.PARAM_TYPES["proc_open"],
            ["array|string", "array", "array", "?string", "?array", "?array"],
        )
        self.assertEqual(
            registry.RETURN_TYPE_OVERRIDES["proc_open"],
            "resource|false",
        )
        self.assertEqual(
            registry.RETURN_TYPE_OVERRIDES["proc_get_status"],
            "array|false",
        )

    def test_documentation_metadata_is_serialized(self) -> None:
        """Carries structured notes and examples into the renderer's JSON schema."""
        builtin = registry.Builtin(
            name="proc_open",
            canonical_name="proc_open",
            area="Process",
            sub_area="Process",
            in_catalog=True,
            is_internal=False,
            examples=["example"],
            see_also=["proc_close"],
            notes=["Windows behavior"],
        )

        serialized = extract._builtin_to_dict(builtin)

        self.assertEqual(serialized["examples"], ["example"])
        self.assertEqual(serialized["see_also"], ["proc_close"])
        self.assertEqual(serialized["notes"], ["Windows behavior"])


if __name__ == "__main__":
    unittest.main()
