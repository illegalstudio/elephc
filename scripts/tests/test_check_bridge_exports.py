#!/usr/bin/env python3
"""Structural tests for the bridge static-archive export verifier."""

from __future__ import annotations

import importlib.util
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).resolve().parents[1] / "check_bridge_exports.py"
SPEC = importlib.util.spec_from_file_location("check_bridge_exports", SCRIPT)
assert SPEC and SPEC.loader
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class BridgeExportTests(unittest.TestCase):
    """Exercise source and nm parsing without requiring a cross-toolchain."""

    def test_source_exports_accepts_safe_and_unsafe_functions(self) -> None:
        """The source scanner recognizes both safe and unsafe C ABI exports."""
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "lib.rs"
            source.write_text(
                '#[no_mangle]\npub extern "C" fn elephc_safe() {}\n'
                '#[no_mangle]\npub unsafe extern "C" fn elephc_unsafe() {}\n'
                'pub extern "C" fn elephc_mangled() {}\n',
                encoding="utf-8",
            )
            self.assertEqual(
                MODULE.source_exports(Path(directory)),
                {"elephc_safe", "elephc_unsafe"},
            )

    def test_nm_symbol_regex_accepts_coff_decoration(self) -> None:
        """The nm parser accepts undecorated x64 and leading-underscore symbols."""
        self.assertEqual(
            MODULE.NM_SYMBOL_RE.match("elephc_tls_read").group(1),
            "elephc_tls_read",
        )
        self.assertEqual(
            MODULE.NM_SYMBOL_RE.match("_elephc_tls_read").group(1),
            "elephc_tls_read",
        )
        self.assertIsNone(MODULE.NM_SYMBOL_RE.match("rust_eh_personality"))

    def test_archive_path_accepts_msvc_staticlibs(self) -> None:
        """Cargo's native MSVC .lib naming is recognized alongside GNU .a."""
        with tempfile.TemporaryDirectory() as directory:
            staticlib = Path(directory) / "debug" / "elephc_tls.lib"
            staticlib.parent.mkdir()
            staticlib.touch()
            self.assertEqual(
                MODULE.archive_path(Path(directory), "", "debug", "tls"),
                staticlib,
            )


if __name__ == "__main__":
    unittest.main()
