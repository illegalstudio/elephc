"""Offline tests for configured php-src stream source reachability."""

from __future__ import annotations

import importlib.util
import json
import subprocess
import tempfile
import unittest
from unittest import mock
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
MODULE_PATH = ROOT / "tools" / "php_oracle" / "extract_reachability.py"
SPEC = importlib.util.spec_from_file_location("extract_reachability", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
extract_reachability = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(extract_reachability)

REACHABILITY_PATH = (
    ROOT
    / "tests"
    / "php_oracle"
    / "reachability"
    / "streams"
    / "php-8.5.6"
    / "macos-aarch64"
    / "streams-full.json"
)
SUPPORTED_TARGETS = ("macos-aarch64", "linux-aarch64", "linux-x86_64")


class ReachabilityTests(unittest.TestCase):
    """Verify provenance, complete classification, and mandatory stream consumers."""

    @classmethod
    def setUpClass(cls) -> None:
        """Load the checked-in configured-build reachability artifact."""
        cls.content = REACHABILITY_PATH.read_bytes()
        cls.manifest = json.loads(cls.content)

    def test_artifact_is_canonical_and_source_pinned(self) -> None:
        """The reachability artifact is canonical and tied to the frozen tree/build."""
        self.assertEqual(
            self.content,
            extract_reachability.canonical_bytes(self.manifest),
        )
        profile = self.manifest["profile"]
        self.assertEqual(profile["php_release"], "8.5.6")
        self.assertEqual(
            profile["php_src_commit"],
            "fcc29c8d6d6ee6f5ba2d941f0a2a6ea6aa6ee633",
        )
        self.assertEqual(profile["php_src_tree"], "25cac7acbc3908cb38a8bfab6be326bdccd9d151")
        self.assertEqual(profile["target"], "macos-aarch64")
        self.assertEqual(profile["build_profile"], "streams-full")

    def test_every_public_handler_has_a_final_classification(self) -> None:
        """No configured handler remains silently unclassified."""
        summary = self.manifest["analysis"]["summary"]
        self.assertEqual(summary["public_entries"], 1980)
        self.assertEqual(summary["unresolved_indirect_entries"], 0)
        self.assertEqual(
            summary["public_entries"],
            summary["direct_stream_entries"] + summary["no_stream_entries"],
        )
        self.assertEqual(self.manifest["gate"]["status"], "candidate")
        self.assertEqual(self.manifest["gate"]["open_requirements"], [])

    def test_replay_parses_then_removes_each_temporary_llvm_module(self) -> None:
        """LLVM replay must not retain every large module until the full build completes."""
        with tempfile.TemporaryDirectory() as name:
            ir_dir = Path(name)
            record = {
                "arguments": ["-c", "fixture.c", "-o", "fixture.o"],
                "directory": name,
            }

            def fake_run(command: list[str], **_: object) -> subprocess.CompletedProcess:
                """Emit one deterministic LLVM module in place of invoking Clang."""
                output = Path(command[command.index("-o") + 1])
                output.write_text(
                    "define void @fixture() {\n"
                    "  call void @php_stream_open_wrapper_ex()\n"
                    "  ret void\n"
                    "}\n"
                )
                return subprocess.CompletedProcess(command, 0, b"", b"")

            with mock.patch.object(
                extract_reachability.subprocess,
                "run",
                side_effect=fake_run,
            ):
                index, graph, indirect, definitions = (
                    extract_reachability.replay_record(
                        (7, record),
                        Path("/usr/bin/clang"),
                        ir_dir,
                    )
                )

            self.assertEqual(index, 7)
            self.assertEqual(
                graph["fixture"],
                {"php_stream_open_wrapper_ex"},
            )
            self.assertEqual(indirect["fixture"], 0)
            self.assertEqual(definitions, {"fixture"})
            self.assertEqual(list(ir_dir.iterdir()), [])

    def test_supported_targets_have_complete_source_reachability(self) -> None:
        """Every supported target classifies the configured public handler graph."""
        root = REACHABILITY_PATH.parents[1]
        for target in SUPPORTED_TARGETS:
            with self.subTest(target=target):
                path = root / target / "streams-full.json"
                content = path.read_bytes()
                manifest = json.loads(content)
                self.assertEqual(
                    content,
                    extract_reachability.canonical_bytes(manifest),
                )
                self.assertEqual(manifest["profile"]["target"], target)
                self.assertTrue(manifest["profile"]["directives_preprocessor"])
                self.assertTrue(
                    manifest["profile"]["directives_preprocessor_version"]
                )
                if target == "linux-x86_64":
                    self.assertIn(
                        "gcc",
                        Path(
                            manifest["profile"]["directives_preprocessor"]
                        ).name,
                    )
                summary = manifest["analysis"]["summary"]
                self.assertEqual(summary["public_entries"], 1980)
                self.assertEqual(summary["unresolved_indirect_entries"], 0)
                self.assertEqual(
                    summary["public_entries"],
                    summary["direct_stream_entries"]
                    + summary["no_stream_entries"],
                )
                self.assertEqual(manifest["gate"]["status"], "candidate")

    def test_required_non_obvious_function_consumers_are_reachable(self) -> None:
        """Core, aliases, compression, hashing, SPL-adjacent, and socket APIs stay in scope."""
        functions = {entry["name"]: entry for entry in self.manifest["functions"]}
        for name in (
            "fopen",
            "fgetcsv",
            "fputcsv",
            "stream_context_create",
            "stream_filter_append",
            "stream_socket_client",
            "socket_set_block",
            "hash_file",
            "getimagesize",
            "opcache_compile_file",
            "gzopen",
            "bzopen",
        ):
            self.assertIn(name, functions)
            self.assertEqual(functions[name]["path"][0], functions[name]["handler"])

    def test_raw_generated_function_aliases_resolve_to_canonical_handlers(self) -> None:
        """Aliases emitted as raw entries retain their php-src canonical target."""
        functions = {entry["name"]: entry for entry in self.manifest["functions"]}
        for alias, canonical in {
            "fputs": "fwrite",
            "set_file_buffer": "stream_set_write_buffer",
            "socket_get_status": "stream_get_meta_data",
            "stream_register_wrapper": "stream_wrapper_register",
        }.items():
            self.assertEqual(functions[alias]["alias_of"], canonical)
            self.assertEqual(functions[alias]["handler"], functions[canonical]["handler"])

    def test_raw_generated_method_aliases_resolve_to_canonical_handlers(self) -> None:
        """Stream-facing class aliases preserve their class-qualified target."""
        classes = {entry["name"]: entry for entry in self.manifest["classes"]}
        methods = {
            method["name"]: method
            for method in classes["SplFileObject"]["methods"]
        }
        self.assertEqual(
            methods["getCurrentLine"]["alias_of"],
            "SplFileObject::fgets",
        )
        self.assertEqual(
            methods["getCurrentLine"]["handler"],
            methods["fgets"]["handler"],
        )

    def test_required_stream_classes_are_selected_by_reachability_or_protocol(self) -> None:
        """Stream protocol classes and stream-backed SPL classes remain in the surface."""
        classes = {entry["name"] for entry in self.manifest["classes"]}
        for name in (
            "php_user_filter",
            "StreamBucket",
            "SplFileInfo",
            "SplFileObject",
            "SplTempFileObject",
            "DirectoryIterator",
            "Phar",
            "ZipArchive",
        ):
            self.assertIn(name, classes)
        for unrelated in (
            "FilterIterator",
            "CallbackFilterIterator",
            "RecursiveFilterIterator",
        ):
            self.assertNotIn(unrelated, classes)


if __name__ == "__main__":
    unittest.main()
