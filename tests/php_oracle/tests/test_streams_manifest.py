"""Offline tests for checked-in PHP 8.5.6 stream build manifests."""

from __future__ import annotations

import copy
import importlib.util
import json
import unittest
from unittest import mock
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
MODULE_PATH = ROOT / "tools" / "php_oracle" / "streams_manifest.py"
SPEC = importlib.util.spec_from_file_location("streams_manifest", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
streams_manifest = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(streams_manifest)

PROFILE_PATH = (
    ROOT
    / "tests"
    / "php_oracle"
    / "manifests"
    / "streams"
    / "php-8.5.6"
    / "macos-aarch64"
    / "homebrew-no-ini.json"
)
SOURCE_PROFILE_PATHS = tuple(
    PROFILE_PATH.parents[1] / target / "streams-full.json"
    for target in streams_manifest.SUPPORTED_TARGETS
)


class StreamsManifestTests(unittest.TestCase):
    """Verify provenance, canonical encoding, and audited blocker constants."""

    def setUp(self) -> None:
        """Load the selected checked-in profile."""
        self.manifest = json.loads(PROFILE_PATH.read_bytes())

    def test_profile_is_canonical_and_structurally_valid(self) -> None:
        """The artifact must round-trip byte-for-byte and satisfy schema checks."""
        self.assertEqual(
            PROFILE_PATH.read_bytes(),
            streams_manifest.canonical_bytes(self.manifest),
        )
        self.assertEqual(
            streams_manifest.validate_manifest(PROFILE_PATH, self.manifest),
            [],
        )

    def test_validation_rejects_hidden_missing_companion_evidence(self) -> None:
        """A hand-edited candidate cannot hide absent reachability, drift, or corpus data."""
        manifest = copy.deepcopy(json.loads(SOURCE_PROFILE_PATHS[0].read_bytes()))
        manifest["reachability"] = None
        manifest["companion_evidence"] = {}
        manifest["gate"]["open_requirements"] = []
        manifest["gate"]["status"] = "candidate"
        errors = streams_manifest.validate_manifest(
            SOURCE_PROFILE_PATHS[0],
            manifest,
        )
        self.assertIn(
            "authoritative-clang-source-reachability is hidden without checked-in evidence",
            errors,
        )
        self.assertIn(
            "elephc-classified-drift-ledger is hidden without checked-in evidence",
            errors,
        )
        self.assertIn(
            "differential-oracle-corpus is hidden without checked-in evidence",
            errors,
        )

    def test_frozen_php_and_source_provenance(self) -> None:
        """The selected profile must remain pinned to PHP/php-src 8.5.6."""
        profile = self.manifest["profile"]
        oracle = self.manifest["oracle"]
        self.assertEqual(profile["php_release"], "8.5.6")
        self.assertEqual(
            profile["php_src_commit"],
            "fcc29c8d6d6ee6f5ba2d941f0a2a6ea6aa6ee633",
        )
        self.assertEqual(profile["target"], "macos-aarch64")
        self.assertEqual(oracle["php_version"], "8.5.6")
        self.assertEqual(oracle["os_family"], "Darwin")
        self.assertIn(oracle["uname_machine"].lower(), {"arm64", "aarch64"})

    def test_linked_library_evidence_normalizes_ephemeral_paths_and_addresses(
        self,
    ) -> None:
        """Dynamic-link provenance must not bind artifacts to a temporary build path."""
        php_binary = Path("/tmp/transient-build/sapi/cli/php")

        def fake_command(arguments: list[str]) -> str:
            """Return deterministic tool output for provenance capture."""
            if arguments[0] == "otool":
                return f"{php_binary}:\n\t/usr/lib/libSystem.B.dylib"
            if arguments[0] == "ldd":
                return (
                    f"{php_binary} (0x1234abcd)\n"
                    "\tlibc.so.6 => /lib/libc.so.6 (0x9876fedc)"
                )
            return "tool-version"

        with (
            mock.patch.object(streams_manifest, "command_output", side_effect=fake_command),
            mock.patch.object(streams_manifest.sys, "platform", "darwin"),
        ):
            macos = streams_manifest.source_build_environment(php_binary)
        self.assertEqual(macos["linked_libraries"][0], "${PHP_BINARY}:")

        with (
            mock.patch.object(streams_manifest, "command_output", side_effect=fake_command),
            mock.patch.object(streams_manifest.sys, "platform", "linux"),
        ):
            linux = streams_manifest.source_build_environment(php_binary)
        linked = "\n".join(linux["linked_libraries"])
        self.assertNotIn(str(php_binary), linked)
        self.assertNotIn("0x1234abcd", linked)
        self.assertNotIn("0x9876fedc", linked)
        self.assertIn("${PHP_BINARY}", linked)
        self.assertEqual(linked.count("${ADDRESS}"), 2)

    def test_configured_capability_order_is_frozen(self) -> None:
        """Wrapper, transport, and filter registry order is observable evidence."""
        surface = self.manifest["surface"]
        self.assertEqual(
            surface["wrappers"],
            [
                "https",
                "ftps",
                "compress.zlib",
                "compress.bzip2",
                "php",
                "file",
                "glob",
                "data",
                "http",
                "ftp",
                "phar",
                "zip",
            ],
        )
        self.assertEqual(
            surface["transports"],
            [
                "tcp",
                "udp",
                "unix",
                "udg",
                "ssl",
                "tls",
                "tlsv1.0",
                "tlsv1.1",
                "tlsv1.2",
                "tlsv1.3",
            ],
        )
        self.assertEqual(
            surface["filters"],
            [
                "zlib.*",
                "bzip2.*",
                "convert.iconv.*",
                "string.rot13",
                "string.toupper",
                "string.tolower",
                "convert.*",
                "consumed",
                "dechunk",
            ],
        )

    def test_audited_constant_blockers_are_exact(self) -> None:
        """The PR's swapped client values and leaked internal names stay visible."""
        constants = self.manifest["surface"]["constants"]
        for name, value in {
            "STREAM_CLIENT_PERSISTENT": 1,
            "STREAM_CLIENT_ASYNC_CONNECT": 2,
            "STREAM_CLIENT_CONNECT": 4,
            "FILE_BINARY": 0,
            "FILE_TEXT": 0,
        }.items():
            self.assertEqual(constants[name], {"type": "int", "value": value})
        for name in (
            "STREAM_FROM_START",
            "STREAM_FROM_CUR",
            "STREAM_FROM_END",
            "STREAM_META_MODIFIED",
            "STREAM_OPTION_CHUNK_SIZE",
        ):
            self.assertNotIn(name, constants)

    def test_user_wrapper_protocol_is_extracted_from_frozen_php_src(self) -> None:
        """All php-src user-wrapper callbacks retain arity and reference evidence."""
        manifest = json.loads(SOURCE_PROFILE_PATHS[0].read_bytes())
        protocol = manifest["wrapper_protocol"]
        callbacks = {callback["name"]: callback for callback in protocol["callbacks"]}
        self.assertEqual(
            set(callbacks),
            {
                "stream_open",
                "stream_close",
                "stream_read",
                "stream_write",
                "stream_flush",
                "stream_seek",
                "stream_tell",
                "stream_eof",
                "stream_stat",
                "url_stat",
                "unlink",
                "rename",
                "mkdir",
                "rmdir",
                "dir_opendir",
                "dir_readdir",
                "dir_rewinddir",
                "dir_closedir",
                "stream_lock",
                "stream_cast",
                "stream_set_option",
                "stream_truncate",
                "stream_metadata",
            },
        )
        invocation = callbacks["stream_open"]["invocations"][0]
        self.assertEqual(invocation["arity"], 4)
        self.assertEqual(
            [
                argument["position"]
                for argument in invocation["arguments"]
                if argument["by_reference"]
            ],
            [3],
        )

    def test_function_and_method_aliases_are_source_exact(self) -> None:
        """Raw arginfo aliases retain their canonical function or method target."""
        manifest = json.loads(SOURCE_PROFILE_PATHS[0].read_bytes())
        functions = {
            function["canonical_name"]: function
            for function in manifest["surface"]["functions"]
        }
        self.assertEqual(functions["fputs"]["alias_of"], "fwrite")
        self.assertEqual(
            functions["stream_register_wrapper"]["alias_of"],
            "stream_wrapper_register",
        )
        classes = {
            class_entry["canonical_name"]: class_entry
            for class_entry in manifest["surface"]["classes"]
        }
        methods = {
            method["canonical_name"]: method
            for method in classes["splfileobject"]["methods"]
        }
        self.assertEqual(
            methods["getcurrentline"]["alias_of"],
            "SplFileObject::fgets",
        )

    def test_profile_cannot_be_mistaken_for_gate_zero_acceptance(self) -> None:
        """Incomplete target/reachability/corpus work must stay machine-visible."""
        gate = self.manifest["gate"]
        self.assertEqual(gate["status"], "partial")
        self.assertEqual(
            self.manifest["build"]["binary_source_attestation"],
            "external-unverified",
        )
        self.assertIn(
            "profile-binary-source-attestation",
            gate["open_requirements"],
        )
        self.assertIn(
            "authoritative-clang-source-reachability",
            gate["open_requirements"],
        )
        self.assertIn("differential-oracle-corpus", gate["open_requirements"])
        self.assertIn("elephc-classified-drift-ledger", gate["open_requirements"])

    def test_source_profile_attests_build_and_gate_zero_companions(self) -> None:
        """Every supported target binds a source build, reachability, corpus, and drift."""
        self.assertEqual(
            {path.parent.name for path in SOURCE_PROFILE_PATHS},
            set(streams_manifest.SUPPORTED_TARGETS),
        )
        for path in SOURCE_PROFILE_PATHS:
            with self.subTest(target=path.parent.name):
                manifest = json.loads(path.read_bytes())
                self.assertEqual(
                    path.read_bytes(),
                    streams_manifest.canonical_bytes(manifest),
                )
                self.assertEqual(
                    streams_manifest.validate_manifest(path, manifest),
                    [],
                )
                self.assertEqual(
                    manifest["build"]["binary_source_attestation"],
                    "source-build",
                )
                capture = manifest["build"]["compile_capture"]
                self.assertGreaterEqual(capture["translation_units"], 560)
                self.assertEqual(
                    capture["translation_units"],
                    capture["unique_translation_units"],
                )
                self.assertTrue(capture["compilers"])
                self.assertTrue(manifest["build"]["environment"]["uname"])
                self.assertTrue(manifest["build"]["environment"]["linked_libraries"])
                self.assertIsNotNone(manifest["reachability"])
                self.assertEqual(
                    set(manifest["companion_evidence"]),
                    {"corpus_index", "drift_ledger"},
                )
                self.assertEqual(manifest["gate"]["status"], "candidate")
                self.assertEqual(manifest["gate"]["open_requirements"], [])


if __name__ == "__main__":
    unittest.main()
