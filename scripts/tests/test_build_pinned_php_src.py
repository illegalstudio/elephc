#!/usr/bin/env python3

import hashlib
import json
import os
import shlex
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
SCRIPT = REPO_ROOT / "scripts" / "build-pinned-php-src.sh"
INVENTORY = REPO_ROOT / "docs" / "specs" / "wasm-inventory.json"


def run(*arguments: str, check: bool = False) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [str(SCRIPT), *arguments],
        cwd=REPO_ROOT,
        check=check,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )


def git(repository: Path, *arguments: str) -> str:
    completed = subprocess.run(
        ["git", "-C", str(repository), *arguments],
        check=True,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    return completed.stdout.strip()


def run_shell(
    body: str,
    *arguments: str,
    environment: dict[str, str] | None = None,
) -> subprocess.CompletedProcess[str]:
    command = f"source {shlex.quote(str(SCRIPT))}; {body}"
    return subprocess.run(
        ["bash", "-c", command, "builder-test", *arguments],
        cwd=REPO_ROOT,
        env=environment,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )


class PinnedPhpSrcBuildTests(unittest.TestCase):
    def test_repository_inventory_yields_the_four_canonical_pins(self) -> None:
        completed = run("--verify-pins-only", check=True)
        records = [line.split("\t") for line in completed.stdout.splitlines()]

        self.assertEqual([record[0] for record in records], ["8.2", "8.3", "8.4", "8.5"])
        for profile, tag, tag_object, tag_commit in records:
            self.assertRegex(tag, rf"^php-{profile}\.[0-9]+$")
            self.assertRegex(tag_object, r"^[0-9a-f]{40}$")
            self.assertRegex(tag_commit, r"^[0-9a-f]{40}$")
            self.assertNotEqual(tag_object, tag_commit)

    def test_profile_selects_exactly_one_canonical_pin(self) -> None:
        completed = run("--verify-pins-only", "--profile", "8.4", check=True)
        records = completed.stdout.splitlines()

        self.assertEqual(len(records), 1)
        self.assertTrue(records[0].startswith("8.4\tphp-8.4."))

    def test_invalid_profile_is_rejected_before_fetch(self) -> None:
        completed = run("--verify-pins-only", "--profile", "8.6")

        self.assertNotEqual(completed.returncode, 0)
        self.assertIn("--profile must be one of", completed.stderr)

    def test_missing_profile_is_rejected_before_fetch(self) -> None:
        document = json.loads(INVENTORY.read_text(encoding="utf-8"))
        document["metadata"]["pins"]["php_src"] = [
            pin
            for pin in document["metadata"]["pins"]["php_src"]
            if pin["profile"] != "8.5"
        ]
        with tempfile.TemporaryDirectory(prefix="elephc-pin-test-") as directory:
            inventory = Path(directory) / "inventory.json"
            inventory.write_text(json.dumps(document), encoding="utf-8")
            completed = run(
                "--inventory",
                str(inventory),
                "--verify-pins-only",
            )

        self.assertNotEqual(completed.returncode, 0)
        self.assertIn("expected exactly php-src profiles", completed.stderr)

    def test_legacy_inventory_schema_is_rejected_before_fetch(self) -> None:
        document = json.loads(INVENTORY.read_text(encoding="utf-8"))
        document["metadata"]["schema"] = "elephc.wasm-inventory.v3"
        with tempfile.TemporaryDirectory(prefix="elephc-pin-test-") as directory:
            inventory = Path(directory) / "inventory.json"
            inventory.write_text(json.dumps(document), encoding="utf-8")
            completed = run(
                "--inventory",
                str(inventory),
                "--verify-pins-only",
            )

        self.assertNotEqual(completed.returncode, 0)
        self.assertIn("metadata.schema must be 'elephc.wasm-inventory.v4'", completed.stderr)

    def test_malformed_commit_is_rejected_before_fetch(self) -> None:
        document = json.loads(INVENTORY.read_text(encoding="utf-8"))
        document["metadata"]["pins"]["php_src"][0]["tag_commit"] = "not-a-commit"
        with tempfile.TemporaryDirectory(prefix="elephc-pin-test-") as directory:
            inventory = Path(directory) / "inventory.json"
            inventory.write_text(json.dumps(document), encoding="utf-8")
            completed = run(
                "--inventory",
                str(inventory),
                "--verify-pins-only",
            )

        self.assertNotEqual(completed.returncode, 0)
        self.assertIn("tag_commit must be 40 lowercase hex characters", completed.stderr)

    def test_duplicate_inventory_keys_are_rejected_before_fetch(self) -> None:
        with tempfile.TemporaryDirectory(prefix="elephc-pin-test-") as directory:
            inventory = Path(directory) / "inventory.json"
            inventory.write_text(
                '{"metadata":{},"metadata":{}}',
                encoding="utf-8",
            )
            completed = run(
                "--inventory",
                str(inventory),
                "--verify-pins-only",
            )

        self.assertNotEqual(completed.returncode, 0)
        self.assertIn("duplicate JSON key", completed.stderr)

    def test_mismatched_wasm_specification_pin_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory(prefix="elephc-input-test-") as directory:
            root = Path(directory)
            inventory = root / "inventory.json"
            specification = root / "wasm-compliance.md"
            document = json.loads(INVENTORY.read_text(encoding="utf-8"))
            document["metadata"]["pins"]["wasm_compliance_sha256"] = "0" * 64
            inventory.write_text(json.dumps(document), encoding="utf-8")
            specification.write_text("tampered specification\n", encoding="utf-8")
            completed = run_shell(
                'verify_specification_pin "$1" "$2"',
                str(inventory),
                str(specification),
            )

        self.assertNotEqual(completed.returncode, 0)
        self.assertIn("expected inventory pin", completed.stderr)

    def test_checkout_verification_rejects_tag_object_mismatch(self) -> None:
        with tempfile.TemporaryDirectory(prefix="elephc-provenance-test-") as directory:
            repository = Path(directory) / "repo"
            repository.mkdir()
            git(repository, "init", "--quiet")
            git(repository, "config", "user.name", "Elephc Test")
            git(repository, "config", "user.email", "test@elephc.invalid")
            (repository / "source.txt").write_text("first\n", encoding="utf-8")
            git(repository, "add", "source.txt")
            git(repository, "commit", "--quiet", "-m", "first")
            first = git(repository, "rev-parse", "HEAD")
            git(repository, "tag", "php-8.2.99")
            (repository / "source.txt").write_text("second\n", encoding="utf-8")
            git(repository, "commit", "--quiet", "-am", "second")
            second = git(repository, "rev-parse", "HEAD")
            git(repository, "checkout", "--quiet", "--detach", second)

            completed = self.run_verify_checkout(
                repository, "php-8.2.99", second, second
            )

        self.assertNotEqual(first, second)
        self.assertNotEqual(completed.returncode, 0)
        self.assertIn(
            f"tag php-8.2.99 object is {first}, expected inventory object {second}",
            completed.stderr,
        )

    def test_checkout_verification_rejects_peeled_commit_mismatch(self) -> None:
        with tempfile.TemporaryDirectory(prefix="elephc-provenance-test-") as directory:
            repository = Path(directory) / "repo"
            repository.mkdir()
            git(repository, "init", "--quiet")
            git(repository, "config", "user.name", "Elephc Test")
            git(repository, "config", "user.email", "test@elephc.invalid")
            (repository / "source.txt").write_text("source\n", encoding="utf-8")
            git(repository, "add", "source.txt")
            git(repository, "commit", "--quiet", "-m", "source")
            commit = git(repository, "rev-parse", "HEAD")
            git(repository, "tag", "-a", "php-8.2.99", "-m", "annotated")
            tag_object = git(repository, "rev-parse", "refs/tags/php-8.2.99")
            git(repository, "checkout", "--quiet", "--detach", commit)
            wrong_commit = "0" * 40

            completed = self.run_verify_checkout(
                repository, "php-8.2.99", tag_object, wrong_commit
            )

        self.assertNotEqual(completed.returncode, 0)
        self.assertIn(
            f"tag php-8.2.99 peels to {commit}, expected inventory commit {wrong_commit}",
            completed.stderr,
        )

    def test_checkout_verification_rejects_dirty_tree(self) -> None:
        with tempfile.TemporaryDirectory(prefix="elephc-provenance-test-") as directory:
            repository = Path(directory) / "repo"
            repository.mkdir()
            git(repository, "init", "--quiet")
            git(repository, "config", "user.name", "Elephc Test")
            git(repository, "config", "user.email", "test@elephc.invalid")
            (repository / "source.txt").write_text("clean\n", encoding="utf-8")
            git(repository, "add", "source.txt")
            git(repository, "commit", "--quiet", "-m", "source")
            commit = git(repository, "rev-parse", "HEAD")
            git(repository, "tag", "php-8.2.99")
            git(repository, "checkout", "--quiet", "--detach", commit)
            (repository / "untracked.txt").write_text("dirty\n", encoding="utf-8")

            completed = self.run_verify_checkout(
                repository, "php-8.2.99", commit, commit
            )

        self.assertNotEqual(completed.returncode, 0)
        self.assertIn("checkout is not clean", completed.stderr)

    def test_annotated_tag_object_and_peeled_commit_are_distinguished(self) -> None:
        with tempfile.TemporaryDirectory(prefix="elephc-provenance-test-") as directory:
            repository = Path(directory) / "repo"
            repository.mkdir()
            git(repository, "init", "--quiet")
            git(repository, "config", "user.name", "Elephc Test")
            git(repository, "config", "user.email", "test@elephc.invalid")
            (repository / "source.txt").write_text("source\n", encoding="utf-8")
            git(repository, "add", "source.txt")
            git(repository, "commit", "--quiet", "-m", "source")
            commit = git(repository, "rev-parse", "HEAD")
            git(repository, "tag", "-a", "php-8.2.99", "-m", "annotated")
            tag_object = git(repository, "rev-parse", "refs/tags/php-8.2.99")
            git(repository, "checkout", "--quiet", "--detach", commit)

            completed = self.run_verify_checkout(
                repository, "php-8.2.99", tag_object, commit
            )

        self.assertNotEqual(tag_object, commit)
        self.assertEqual(completed.returncode, 0, completed.stderr)
        self.assertEqual(
            completed.stdout.strip().split("\t"),
            [tag_object, commit, commit],
        )

    def test_hash_verification_rejects_tampered_provenance(self) -> None:
        with tempfile.TemporaryDirectory(prefix="elephc-provenance-test-") as directory:
            root = Path(directory)
            provenance = root / "provenance.json"
            hashes = root / "hashes.sha256"
            provenance.write_text('{"commit":"first"}\n', encoding="utf-8")
            digest = hashlib.sha256(provenance.read_bytes()).hexdigest()
            hashes.write_text(f"{digest}  provenance.json\n", encoding="utf-8")
            provenance.write_text('{"commit":"tampered"}\n', encoding="utf-8")
            command = (
                f"source {shlex.quote(str(SCRIPT))}; "
                'verify_hash_manifest "$1" "$2"'
            )
            completed = subprocess.run(
                ["bash", "-c", command, "verify-hashes", str(root), str(hashes)],
                cwd=REPO_ROOT,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
            )

        self.assertNotEqual(completed.returncode, 0)
        self.assertIn("SHA-256 mismatch", completed.stderr)

    def test_metadata_environment_has_no_path_proxy_or_token(self) -> None:
        environment = os.environ.copy()
        environment.update(
            {
                "PATH": environment["PATH"],
                "HTTPS_PROXY": "https://proxy.invalid",
                "SECRET_TOKEN": "must-not-leak",
            }
        )
        completed = run_shell(
            'run_metadata_env 123 "$1" -c '
            "'import json, os; print(json.dumps(dict(os.environ), sort_keys=True))'",
            sys.executable,
            environment=environment,
        )

        self.assertEqual(completed.returncode, 0, completed.stderr)
        observed = json.loads(completed.stdout)
        self.assertEqual(observed["LC_ALL"], "C")
        self.assertEqual(observed["SOURCE_DATE_EPOCH"], "123")
        self.assertEqual(observed["TZ"], "UTC")
        self.assertLessEqual(
            set(observed),
            {"LC_ALL", "SOURCE_DATE_EPOCH", "TZ", "__CF_USER_TEXT_ENCODING"},
        )
        for forbidden in ("PATH", "HTTPS_PROXY", "SECRET_TOKEN"):
            self.assertNotIn(forbidden, observed)

    def test_php_metadata_probe_records_targeted_runtime_configuration(self) -> None:
        php = shutil.which("php")
        if php is None:
            self.skipTest("host PHP CLI is unavailable")
        version = subprocess.run(
            [php, "-n", "-r", "fwrite(STDOUT, PHP_VERSION);"],
            check=True,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        ).stdout
        environment = os.environ.copy()
        environment.update(
            {
                "HTTPS_PROXY": "https://proxy.invalid",
                "SECRET_TOKEN": "must-not-leak",
            }
        )
        with tempfile.TemporaryDirectory(prefix="elephc-metadata-test-") as directory:
            metadata = Path(directory) / "php-metadata.json"
            completed = run_shell(
                'write_php_metadata "$1" 123 "$2" "$3"',
                php,
                version,
                str(metadata),
                environment=environment,
            )
            self.assertEqual(completed.returncode, 0, completed.stderr)
            document = json.loads(metadata.read_text(encoding="utf-8"))

        self.assertEqual(document["php"]["version"], version)
        self.assertEqual(document["php"]["sapi"], "cli")
        self.assertIsNone(document["php"]["ini"]["loaded_file"])
        self.assertIsNone(document["php"]["ini"]["scanned_files"])
        self.assertIsInstance(document["php"]["extensions"], list)
        self.assertEqual(
            document["configure"]["args"],
            [
                "--prefix=/install",
                "--disable-all",
                "--enable-cli",
                "--disable-cgi",
                "--disable-phpdbg",
                "--without-pear",
            ],
        )
        for forbidden in ("PATH", "HTTPS_PROXY", "SECRET_TOKEN"):
            self.assertNotIn(forbidden, document["environment"])

    def test_build_environment_is_explicit_and_sensitive_values_are_cleared(self) -> None:
        environment = os.environ.copy()
        environment.update(
            {
                "CC": "clang",
                "CFLAGS": "-O2 -g0",
                "CONFIG_SITE": "/tmp/host-config.site",
                "MAKEFLAGS": "-j99",
                "HTTPS_PROXY": "https://proxy.invalid",
                "SECRET_TOKEN": "must-not-leak",
            }
        )
        with tempfile.TemporaryDirectory(prefix="elephc-env-test-") as directory:
            manifest = Path(directory) / "build-environment.json"
            completed = run_shell(
                'prepare_build_environment; '
                'write_build_environment_manifest "$1"; '
                'run_build_env "$2" -c '
                "'import json, os; print(json.dumps(dict(os.environ), sort_keys=True))'",
                str(manifest),
                sys.executable,
                environment=environment,
            )
            self.assertEqual(completed.returncode, 0, completed.stderr)
            observed = json.loads(completed.stdout)
            document = json.loads(manifest.read_text(encoding="utf-8"))

        self.assertEqual(observed["CC"], "clang")
        self.assertEqual(observed["CFLAGS"], "-O2 -g0")
        self.assertIn("PATH", observed)
        for forbidden in (
            "CONFIG_SITE",
            "MAKEFLAGS",
            "HTTPS_PROXY",
            "SECRET_TOKEN",
        ):
            self.assertNotIn(forbidden, observed)
        self.assertEqual(
            document["toolchain_overrides"]["CC"],
            {"set": True, "value": "clang"},
        )
        self.assertEqual(
            document["toolchain_overrides"]["CFLAGS"],
            {"set": True, "value": "-O2 -g0"},
        )
        self.assertIn("CONFIG_SITE", document["explicitly_cleared"])
        self.assertIn("MAKEFLAGS", document["explicitly_cleared"])
        self.assertTrue(Path(document["resolved_tools"]["bison"]).is_absolute())
        self.assertTrue(Path(document["resolved_tools"]["cc"]).is_absolute())
        for forbidden in (
            "CONFIG_SITE",
            "MAKEFLAGS",
            "HTTPS_PROXY",
            "SECRET_TOKEN",
        ):
            self.assertNotIn(forbidden, document["observed"])

    def test_unsupported_bison_version_is_rejected_before_fetch(self) -> None:
        with tempfile.TemporaryDirectory(prefix="elephc-tool-test-") as directory:
            binary_directory = Path(directory) / "bin"
            binary_directory.mkdir()
            bison = binary_directory / "bison"
            bison.write_text(
                "#!/bin/sh\nprintf '%s\\n' 'bison (GNU Bison) 2.3'\n",
                encoding="utf-8",
            )
            bison.chmod(0o755)
            completed = run_shell(
                'BUILD_ENV_READY=1; '
                'BUILD_ENV_ASSIGNMENTS=("PATH=$1" "LC_ALL=C" "TZ=UTC"); '
                "verify_build_tool_versions",
                str(binary_directory),
            )

        self.assertNotEqual(completed.returncode, 0)
        self.assertIn("bison 3.0 or later is required", completed.stderr)

    def test_install_tree_manifest_records_hashes_modes_and_symlink_targets(self) -> None:
        with tempfile.TemporaryDirectory(prefix="elephc-tree-test-") as directory:
            root = Path(directory)
            install = root / "install"
            binary = install / "bin" / "php"
            data = install / "lib" / "data.txt"
            binary.parent.mkdir(parents=True)
            data.parent.mkdir(parents=True)
            binary.write_bytes(b"php-binary\n")
            data.write_bytes(b"runtime-data\n")
            binary.chmod(0o755)
            data.chmod(0o640)
            (install / "php-link").symlink_to("bin/php")
            manifest = root / "install-tree.json"

            written = run_shell(
                'install_tree_manifest write "$1" "$2"; '
                'install_tree_manifest verify "$1" "$2"',
                str(install),
                str(manifest),
            )
            self.assertEqual(written.returncode, 0, written.stderr)
            document = json.loads(manifest.read_text(encoding="utf-8"))
            entries = {entry["path"]: entry for entry in document["entries"]}

            self.assertEqual(entries["."]["type"], "directory")
            self.assertEqual(entries["bin/php"]["mode"], "0755")
            self.assertEqual(entries["bin/php"]["type"], "file")
            self.assertEqual(
                entries["bin/php"]["sha256"],
                hashlib.sha256(b"php-binary\n").hexdigest(),
            )
            self.assertEqual(entries["lib/data.txt"]["mode"], "0640")
            self.assertEqual(entries["php-link"]["type"], "symlink")
            self.assertEqual(entries["php-link"]["target"], "bin/php")

            data.chmod(0o600)
            tampered = run_shell(
                'install_tree_manifest verify "$1" "$2"',
                str(install),
                str(manifest),
            )

        self.assertNotEqual(tampered.returncode, 0)
        self.assertIn("does not match", tampered.stderr)

    def test_publish_no_clobber_rejects_destination_that_appeared(self) -> None:
        with tempfile.TemporaryDirectory(prefix="elephc-publish-test-") as directory:
            root = Path(directory)
            source = root / "staging" / "result"
            destination = root / "published"
            source.mkdir(parents=True)
            (source / "artifact").write_text("staged\n", encoding="utf-8")

            # Simulate a competing publisher after main's initial destination check.
            destination.mkdir()
            marker = destination / "other-publisher"
            marker.write_text("keep\n", encoding="utf-8")
            completed = run_shell(
                'publish_no_clobber "$1" "$2"',
                str(source),
                str(destination),
            )

            self.assertNotEqual(completed.returncode, 0)
            self.assertIn("already exists", completed.stderr)
            self.assertTrue(source.is_dir())
            self.assertEqual(marker.read_text(encoding="utf-8"), "keep\n")
            self.assertFalse((destination / "result").exists())

    def test_publish_no_clobber_atomically_renames_when_destination_is_absent(self) -> None:
        with tempfile.TemporaryDirectory(prefix="elephc-publish-test-") as directory:
            root = Path(directory)
            source = root / "staging" / "result"
            destination = root / "published"
            source.mkdir(parents=True)
            (source / "artifact").write_text("staged\n", encoding="utf-8")
            completed = run_shell(
                'publish_no_clobber "$1" "$2"',
                str(source),
                str(destination),
            )

            self.assertEqual(completed.returncode, 0, completed.stderr)
            self.assertFalse(source.exists())
            self.assertEqual(
                (destination / "artifact").read_text(encoding="utf-8"),
                "staged\n",
            )

    def test_profile_provenance_records_both_git_objects(self) -> None:
        tag_object = "1" * 40
        tag_commit = "2" * 40
        digest = "a" * 64
        with tempfile.TemporaryDirectory(prefix="elephc-provenance-test-") as directory:
            root = Path(directory)
            profile_root = root / "8.2"
            profile_root.mkdir()
            destination = profile_root / "provenance.json"
            (profile_root / "php-metadata.json").write_text(
                json.dumps(
                    {
                        "php": {
                            "version": "8.2.99",
                            "ini": {
                                "loaded_file": None,
                                "scanned_files": None,
                                "config_file_path": "/install/lib",
                                "config_file_scan_dir": "",
                            },
                            "extensions": ["Core", "json"],
                            "zend_extensions": [],
                        },
                        "configure": {
                            "args": ["--prefix=/install", "--disable-all"],
                        },
                    }
                ),
                encoding="utf-8",
            )
            (root / "build-environment.json").write_text(
                json.dumps(
                    {
                        "toolchain_overrides": {
                            name: {"set": False, "value": None}
                            for name in (
                                "CC",
                                "CFLAGS",
                                "CPPFLAGS",
                                "LDFLAGS",
                                "PKG_CONFIG_PATH",
                            )
                        }
                    }
                ),
                encoding="utf-8",
            )
            command = (
                f"source {shlex.quote(str(SCRIPT))}; "
                'write_profile_provenance "$1" 8.2 php-8.2.99 "$2" "$3" '
                '"$2" "$3" "$3" "$4" "$4" "$4" "$4" '
                '8.2.99 123 "$4" "$4" "$4" "$4" '
                '"git test" "autoconf test" "bison test" "re2c test" '
                '"make test" "cc test" --prefix=/install --disable-all'
            )
            completed = subprocess.run(
                [
                    "bash",
                    "-c",
                    command,
                    "write-provenance",
                    str(destination),
                    tag_object,
                    tag_commit,
                    digest,
                ],
                cwd=REPO_ROOT,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
            )
            self.assertEqual(completed.returncode, 0, completed.stderr)
            document = json.loads(destination.read_text(encoding="utf-8"))

        self.assertEqual(document["source"]["inventory_tag_object"], tag_object)
        self.assertEqual(document["source"]["inventory_tag_commit"], tag_commit)
        self.assertEqual(document["source"]["tag_object"], tag_object)
        self.assertEqual(document["source"]["tag_commit"], tag_commit)
        self.assertEqual(document["source"]["head"], tag_commit)
        self.assertEqual(document["schema"], "elephc.pinned-php-src-build.v2")
        self.assertEqual(document["inputs"]["inventory_sha256"], digest)
        self.assertEqual(document["inputs"]["wasm_specification_sha256"], digest)
        self.assertEqual(document["inputs"]["builder_script_sha256"], digest)
        self.assertEqual(document["artifact"]["install_tree_sha256"], digest)
        self.assertEqual(document["artifact"]["runtime_metadata_sha256"], digest)
        self.assertEqual(document["artifact"]["dynamic_dependencies_sha256"], digest)
        self.assertEqual(
            document["build"]["configure_command"],
            ["configure", "--prefix=/install", "--disable-all"],
        )
        self.assertEqual(
            document["build"]["build_flags"],
            {
                "CC": None,
                "CFLAGS": None,
                "CPPFLAGS": None,
                "LDFLAGS": None,
                "PKG_CONFIG_PATH": None,
            },
        )
        self.assertEqual(document["runtime"]["ini_mode"], "-n")
        self.assertEqual(document["runtime"]["extensions"], ["Core", "json"])
        self.assertEqual(
            document["build"]["configure_args"],
            ["--prefix=/install", "--disable-all"],
        )

    @staticmethod
    def run_verify_checkout(
        repository: Path, tag: str, tag_object: str, tag_commit: str
    ) -> subprocess.CompletedProcess[str]:
        command = (
            f"source {shlex.quote(str(SCRIPT))}; "
            'verify_checkout "$1" "$2" "$3" "$4"'
        )
        return subprocess.run(
            [
                "bash",
                "-c",
                command,
                "verify-checkout",
                str(repository),
                tag,
                tag_object,
                tag_commit,
            ],
            cwd=REPO_ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )


if __name__ == "__main__":
    unittest.main()
