"""Independent validation tests for published pinned php-src build artifacts."""

from __future__ import annotations

import hashlib
import json
import os
import platform
import stat
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from scripts.wasm_oracle import (
    ContractError,
    OracleContract,
    load_php_src_runtime_artifact,
    sha256_file,
)


REPO_ROOT = Path(__file__).resolve().parents[2]
ELEPHC_SOURCE_COMMIT = "a" * 40
PROFILE = "8.2"


def write_json(path: Path, value: object) -> None:
    """Write canonical builder-style JSON with one trailing newline."""

    path.write_text(
        json.dumps(value, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )


def install_entry(root: Path, path: Path) -> dict[str, object]:
    """Describe one install-tree entry exactly like the pinned builder."""

    relative = path.relative_to(root).as_posix()
    details = path.lstat()
    mode = f"{stat.S_IMODE(details.st_mode):04o}"
    if stat.S_ISDIR(details.st_mode):
        return {"path": relative, "type": "directory", "mode": mode}
    if stat.S_ISREG(details.st_mode):
        return {
            "path": relative,
            "type": "file",
            "mode": mode,
            "size": details.st_size,
            "sha256": sha256_file(path),
        }
    if stat.S_ISLNK(details.st_mode):
        return {
            "path": relative,
            "type": "symlink",
            "mode": mode,
            "target": os.readlink(path),
        }
    raise AssertionError(f"unsupported fixture entry: {path}")


def write_install_manifest(profile_root: Path) -> None:
    """Freeze the fixture install tree including modes, hashes, and symlinks."""

    install = profile_root / "install"
    paths = sorted(
        install.rglob("*"),
        key=lambda item: item.relative_to(install).as_posix(),
    )
    write_json(
        profile_root / "install-tree.json",
        {
            "schema": "elephc.pinned-php-src-install-tree.v1",
            "root": "install",
            "entries": [
                install_entry(install, install),
                *(install_entry(install, path) for path in paths),
            ],
        },
    )


def write_hash_manifest(
    root: Path, destination: Path, relative_paths: tuple[str, ...]
) -> None:
    """Hash an exact ordered path set using the builder manifest encoding."""

    destination.write_text(
        "".join(
            f"{sha256_file(root / relative)}  {relative}\n"
            for relative in relative_paths
        ),
        encoding="utf-8",
    )


def runtime_probe(version: str) -> dict[str, object]:
    """Return the expected result of executing the fixture PHP runtime with `-n`."""

    return {
        "version": version,
        "sapi": "cli",
        "loaded_ini": None,
        "scanned_ini": None,
        "extensions": ["Core", "date", "json"],
        "zend_extensions": [],
    }


class PhpSrcArtifactFixture:
    """Builds a synthetic but schema-complete pinned php-src artifact set."""

    def __init__(self, root: Path, contract: OracleContract) -> None:
        self.root = root
        self.contract = contract
        self.pin = contract.php_src_pin(PROFILE)
        self.profile_root = root / PROFILE
        self.profile_root.mkdir(parents=True)
        self._write_static_files()
        self._write_profile_provenance()
        self._write_profile_hashes()
        self._write_root_provenance()
        self._write_root_hashes()

    def _write_static_files(self) -> None:
        """Create the executable, manifests, metadata, and deterministic inputs."""

        build_environment = {
            "schema": "elephc.pinned-php-src-build-environment.v1",
            "passed": {
                "PATH": "/usr/bin:/bin",
                "LC_ALL": "C",
                "TZ": "UTC",
                "GIT_CONFIG_NOSYSTEM": "1",
                "GIT_CONFIG_GLOBAL": "/dev/null",
                "GIT_TERMINAL_PROMPT": "0",
            },
            "toolchain_overrides": {
                name: {"set": False, "value": None}
                for name in (
                    "CC",
                    "CFLAGS",
                    "CPPFLAGS",
                    "LDFLAGS",
                    "PKG_CONFIG_PATH",
                )
            },
            "resolved_tools": {
                name: f"/usr/bin/{name}"
                for name in ("git", "tar", "autoconf", "bison", "re2c", "make", "cc")
            },
            "observed": {
                "PATH": "/usr/bin:/bin",
                "LC_ALL": "C",
                "TZ": "UTC",
                "GIT_CONFIG_NOSYSTEM": "1",
                "GIT_CONFIG_GLOBAL": "/dev/null",
                "GIT_TERMINAL_PROMPT": "0",
            },
            "platform_injected_allowed": [],
            "explicitly_cleared": sorted([
                "BASH_ENV",
                "CONFIG_SITE",
                "ENV",
                "GIT_ASKPASS",
                "GIT_CONFIG_COUNT",
                "GIT_SSH",
                "GIT_SSH_COMMAND",
                "HTTP_PROXY",
                "HTTPS_PROXY",
                "LD_LIBRARY_PATH",
                "LIBRARY_PATH",
                "MAKEFLAGS",
                "MAKELEVEL",
                "MFLAGS",
                "NO_PROXY",
                "PHP_INI_SCAN_DIR",
                "all_proxy",
                "http_proxy",
                "https_proxy",
                "no_proxy",
            ]),
            "source_date_epoch": "recorded per profile",
        }
        write_json(self.root / "build-environment.json", build_environment)
        (self.root / "elephc-git-status.txt").write_text(
            "", encoding="utf-8"
        )

        executable = self.profile_root / "install" / "bin" / "php"
        executable.parent.mkdir(parents=True)
        version = self.pin.tag.removeprefix("php-")
        probe_json = json.dumps(runtime_probe(version), separators=(",", ":"))
        executable.write_text(
            "#!/bin/sh\n"
            '[ "$1" = "-n" ] || exit 91\n'
            '[ "$2" = "-r" ] || exit 92\n'
            '[ "$LC_ALL" = "C" ] || exit 93\n'
            '[ "$TZ" = "UTC" ] || exit 94\n'
            '[ "$SOURCE_DATE_EPOCH" = "123" ] || exit 95\n'
            f"printf '%s' '{probe_json}'\n",
            encoding="utf-8",
        )
        executable.chmod(0o755)
        library = self.profile_root / "install" / "lib" / "php" / "core.dat"
        library.parent.mkdir(parents=True)
        library.write_bytes(b"pinned runtime data\n")
        library.chmod(0o640)
        (self.profile_root / "install" / "php-link").symlink_to("bin/php")
        write_install_manifest(self.profile_root)

        ini = {
            "loaded_file": None,
            "scanned_files": None,
            "config_file_path": False,
            "config_file_scan_dir": False,
        }
        write_json(
            self.profile_root / "php-metadata.json",
            {
                "schema": "elephc.pinned-php-src-runtime-metadata.v1",
                "environment": {
                    "LC_ALL": "C",
                    "SOURCE_DATE_EPOCH": "123",
                    "TZ": "UTC",
                },
                "environment_policy": {
                    "passed": {
                        "LC_ALL": "C",
                        "SOURCE_DATE_EPOCH": "123",
                        "TZ": "UTC",
                    },
                    "platform_injected_allowed": [],
                },
                "php": {
                    "version": version,
                    "sapi": "cli",
                    "ini": ini,
                    "extensions": ["Core", "date", "json"],
                    "zend_extensions": [],
                },
                "configure": {
                    "args": [
                        "--prefix=/install",
                        "--disable-all",
                        "--enable-cli",
                        "--disable-cgi",
                        "--disable-phpdbg",
                        "--without-pear",
                    ],
                    "command": [
                        "configure",
                        "--prefix=/install",
                        "--disable-all",
                        "--enable-cli",
                        "--disable-cgi",
                        "--disable-phpdbg",
                        "--without-pear",
                    ],
                },
            },
        )
        platform_name = platform.system()
        dependency_tool = {"Darwin": "otool -L", "Linux": "ldd"}[platform_name]
        (self.profile_root / "dynamic-dependencies.txt").write_text(
            f"platform: {platform_name}\n"
            f"tool: {dependency_tool}\n"
            "binary: install/bin/php\n"
            "output:\n"
            "synthetic dependency evidence\n",
            encoding="utf-8",
        )

    def _write_profile_provenance(self) -> None:
        """Write profile provenance anchored to current contract inputs."""

        version = self.pin.tag.removeprefix("php-")
        configure_args = [
            "--prefix=/install",
            "--disable-all",
            "--enable-cli",
            "--disable-cgi",
            "--disable-phpdbg",
            "--without-pear",
        ]
        build_flags = {
            "CC": None,
            "CFLAGS": None,
            "CPPFLAGS": None,
            "LDFLAGS": None,
            "PKG_CONFIG_PATH": None,
        }
        ini = {
            "loaded_file": None,
            "scanned_files": None,
            "config_file_path": False,
            "config_file_scan_dir": False,
        }
        write_json(
            self.profile_root / "provenance.json",
            {
                "schema": "elephc.pinned-php-src-build.v2",
                "profile": PROFILE,
                "repository": "https://github.com/php/php-src.git",
                "inputs": {
                    "inventory_sha256": self.contract.inventory_sha256,
                    "wasm_specification_sha256": self.contract.specification_sha256,
                    "builder_script_sha256": sha256_file(
                        REPO_ROOT / "scripts" / "build-pinned-php-src.sh"
                    ),
                    "build_environment": "../build-environment.json",
                    "build_environment_sha256": sha256_file(
                        self.root / "build-environment.json"
                    ),
                },
                "source": {
                    "tag": self.pin.tag,
                    "inventory_tag_object": self.pin.tag_object,
                    "inventory_tag_commit": self.pin.tag_commit,
                    "tag_object": self.pin.tag_object,
                    "tag_commit": self.pin.tag_commit,
                    "peeled_commit": self.pin.tag_commit,
                    "head": self.pin.tag_commit,
                    "detached": True,
                    "dirty": False,
                    "materialization": "git archive of verified HEAD",
                    "source_date_epoch": 123,
                },
                "build": {
                    "configure_args": configure_args,
                    "configure_command": ["configure", *configure_args],
                    "build_flags": build_flags,
                    "environment": "../build-environment.json",
                    "ini_mode": "-n",
                    "tools": {
                        "git": "git version test",
                        "autoconf": "autoconf test",
                        "bison": "bison test",
                        "re2c": "re2c test",
                        "make": "make test",
                        "cc": "cc test",
                    },
                },
                "runtime": {
                    "ini_mode": "-n",
                    "ini": ini,
                    "extensions": ["Core", "date", "json"],
                    "zend_extensions": [],
                },
                "artifact": {
                    "php_binary": "install/bin/php",
                    "php_version": version,
                    "php_sha256": sha256_file(
                        self.profile_root / "install" / "bin" / "php"
                    ),
                    "runtime_metadata": "php-metadata.json",
                    "runtime_metadata_sha256": sha256_file(
                        self.profile_root / "php-metadata.json"
                    ),
                    "install_tree": "install-tree.json",
                    "install_tree_sha256": sha256_file(
                        self.profile_root / "install-tree.json"
                    ),
                    "dynamic_dependencies": "dynamic-dependencies.txt",
                    "dynamic_dependencies_sha256": sha256_file(
                        self.profile_root / "dynamic-dependencies.txt"
                    ),
                },
            },
        )

    def _write_profile_hashes(self) -> None:
        """Write the exact profile hash manifest."""

        write_hash_manifest(
            self.profile_root,
            self.profile_root / "hashes.sha256",
            (
                "install/bin/php",
                "install-tree.json",
                "php-metadata.json",
                "dynamic-dependencies.txt",
                "provenance.json",
            ),
        )

    def _root_profile(self) -> dict[str, object]:
        """Derive the root summary from the profile provenance."""

        document = json.loads(
            (self.profile_root / "provenance.json").read_text(encoding="utf-8")
        )
        return {
            "profile": PROFILE,
            "tag": document["source"]["tag"],
            "tag_object": document["source"]["tag_object"],
            "tag_commit": document["source"]["tag_commit"],
            "peeled_commit": document["source"]["peeled_commit"],
            "php_binary": f"{PROFILE}/{document['artifact']['php_binary']}",
            "php_version": document["artifact"]["php_version"],
            "php_sha256": document["artifact"]["php_sha256"],
            "configure_command": document["build"]["configure_command"],
            "configure_args": document["build"]["configure_args"],
            "build_flags": document["build"]["build_flags"],
            "ini_mode": document["runtime"]["ini_mode"],
            "ini": document["runtime"]["ini"],
            "extensions": document["runtime"]["extensions"],
            "zend_extensions": document["runtime"]["zend_extensions"],
            "install_tree_sha256": document["artifact"]["install_tree_sha256"],
            "runtime_metadata_sha256": document["artifact"][
                "runtime_metadata_sha256"
            ],
            "dynamic_dependencies_sha256": document["artifact"][
                "dynamic_dependencies_sha256"
            ],
            "provenance": f"{PROFILE}/provenance.json",
            "hashes": f"{PROFILE}/hashes.sha256",
        }

    def _write_root_provenance(self, *, dirty: bool = False) -> None:
        """Write build-set provenance and its exact profile summary."""

        write_json(
            self.root / "provenance.json",
            {
                "schema": "elephc.pinned-php-src-build-set.v2",
                "repository": "https://github.com/php/php-src.git",
                "selection": [PROFILE],
                "inputs": {
                    "inventory": {
                        "path": "docs/specs/wasm-inventory.json",
                        "sha256": self.contract.inventory_sha256,
                    },
                    "wasm_specification": {
                        "path": "docs/specs/wasm-compliance.md",
                        "sha256": self.contract.specification_sha256,
                        "inventory_pin_sha256": self.contract.specification_sha256,
                    },
                    "builder_script": {
                        "path": "scripts/build-pinned-php-src.sh",
                        "sha256": sha256_file(
                            REPO_ROOT / "scripts" / "build-pinned-php-src.sh"
                        ),
                    },
                    "build_environment": {
                        "path": "build-environment.json",
                        "sha256": sha256_file(
                            self.root / "build-environment.json"
                        ),
                    },
                    "elephc": {
                        "head": ELEPHC_SOURCE_COMMIT,
                        "dirty": dirty,
                        "status": "elephc-git-status.txt",
                        "status_sha256": sha256_file(
                            self.root / "elephc-git-status.txt"
                        ),
                    },
                },
                "profiles": [self._root_profile()],
            },
        )

    def _write_root_hashes(self) -> None:
        """Write the exact build-set hash manifest."""

        write_hash_manifest(
            self.root,
            self.root / "hashes.sha256",
            (
                "build-environment.json",
                "elephc-git-status.txt",
                "provenance.json",
                f"{PROFILE}/install/bin/php",
                f"{PROFILE}/install-tree.json",
                f"{PROFILE}/php-metadata.json",
                f"{PROFILE}/dynamic-dependencies.txt",
                f"{PROFILE}/provenance.json",
                f"{PROFILE}/hashes.sha256",
            ),
        )

    def resign_profile_and_root(self) -> None:
        """Refresh dependent manifests after a deliberate semantic mutation."""

        self._write_profile_hashes()
        self._write_root_provenance()
        self._write_root_hashes()

    def resign_all(self) -> None:
        """Refresh cross-file hashes after mutating build or artifact evidence."""

        provenance_path = self.profile_root / "provenance.json"
        document = json.loads(provenance_path.read_text(encoding="utf-8"))
        document["inputs"]["build_environment_sha256"] = sha256_file(
            self.root / "build-environment.json"
        )
        document["artifact"]["php_sha256"] = sha256_file(
            self.profile_root / "install" / "bin" / "php"
        )
        document["artifact"]["runtime_metadata_sha256"] = sha256_file(
            self.profile_root / "php-metadata.json"
        )
        document["artifact"]["install_tree_sha256"] = sha256_file(
            self.profile_root / "install-tree.json"
        )
        document["artifact"]["dynamic_dependencies_sha256"] = sha256_file(
            self.profile_root / "dynamic-dependencies.txt"
        )
        write_json(provenance_path, document)
        self.resign_profile_and_root()


class WasmPhpSrcArtifactTests(unittest.TestCase):
    """Exercises successful loading and representative fail-closed mutations."""

    @classmethod
    def setUpClass(cls) -> None:
        cls.contract = OracleContract.load(REPO_ROOT)

    def test_complete_artifact_loads_runtime_provenance(self) -> None:
        """A complete exact build set yields executable and provenance hashes."""

        with tempfile.TemporaryDirectory(prefix="elephc-php-src-artifact-") as tmp:
            fixture = PhpSrcArtifactFixture(Path(tmp), self.contract)
            version = fixture.pin.tag.removeprefix("php-")
            artifact = load_php_src_runtime_artifact(
                fixture.root,
                PROFILE,
                self.contract,
                ELEPHC_SOURCE_COMMIT,
            )

        self.assertEqual(artifact.profile, PROFILE)
        self.assertEqual(artifact.provenance.version, version)
        self.assertEqual(artifact.provenance.source_commit, fixture.pin.tag_commit)
        self.assertEqual(artifact.provenance.ini_mode, "php-n")
        self.assertEqual(artifact.provenance.extensions, ("Core", "date", "json"))
        self.assertRegex(artifact.root_provenance_sha256, r"^[0-9a-f]{64}$")

    def test_cli_prints_verified_php_src_artifact(self) -> None:
        """The public command exposes only evidence accepted by the loader."""

        with tempfile.TemporaryDirectory(prefix="elephc-php-src-artifact-") as tmp:
            fixture = PhpSrcArtifactFixture(Path(tmp), self.contract)
            completed = subprocess.run(
                [
                    sys.executable,
                    str(REPO_ROOT / "scripts" / "wasm_php_oracle.py"),
                    "--repo-root",
                    str(REPO_ROOT),
                    "validate-php-src-build",
                    "--build-root",
                    str(fixture.root),
                    "--profile",
                    PROFILE,
                    "--elephc-source-commit",
                    ELEPHC_SOURCE_COMMIT,
                ],
                cwd=REPO_ROOT,
                check=False,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                timeout=10,
            )

        self.assertEqual(completed.returncode, 0, completed.stderr)
        document = json.loads(completed.stdout)
        self.assertEqual(document["profile"], PROFILE)
        self.assertEqual(
            document["provenance"]["version"],
            self.contract.php_src_pin(PROFILE).tag.removeprefix("php-"),
        )
        self.assertEqual(completed.stderr, "")

    def test_dirty_elephc_build_is_rejected_even_when_rehashed(self) -> None:
        """A self-consistent artifact built from a dirty compiler tree is invalid."""

        with tempfile.TemporaryDirectory(prefix="elephc-php-src-artifact-") as tmp:
            fixture = PhpSrcArtifactFixture(Path(tmp), self.contract)
            fixture._write_root_provenance(dirty=True)
            fixture._write_root_hashes()
            with self.assertRaisesRegex(
                ContractError, "exact clean Elephc SHA"
            ):
                load_php_src_runtime_artifact(
                    fixture.root,
                    PROFILE,
                    self.contract,
                    ELEPHC_SOURCE_COMMIT,
                )

    def test_nonempty_clean_git_status_is_rejected_even_when_rehashed(self) -> None:
        """`dirty:false` cannot be paired with nonempty porcelain output."""

        with tempfile.TemporaryDirectory(prefix="elephc-php-src-artifact-") as tmp:
            fixture = PhpSrcArtifactFixture(Path(tmp), self.contract)
            (fixture.root / "elephc-git-status.txt").write_text(
                " M src/changed.rs\n",
                encoding="utf-8",
            )
            fixture._write_root_provenance()
            fixture._write_root_hashes()
            with self.assertRaisesRegex(ContractError, "empty Git status"):
                load_php_src_runtime_artifact(
                    fixture.root,
                    PROFILE,
                    self.contract,
                    ELEPHC_SOURCE_COMMIT,
                )

    def test_empty_explicit_build_flag_matches_builder_semantics(self) -> None:
        """A defined empty optional build flag remains valid and observable."""

        with tempfile.TemporaryDirectory(prefix="elephc-php-src-artifact-") as tmp:
            fixture = PhpSrcArtifactFixture(Path(tmp), self.contract)
            environment_path = fixture.root / "build-environment.json"
            environment = json.loads(environment_path.read_text(encoding="utf-8"))
            environment["toolchain_overrides"]["CFLAGS"] = {
                "set": True,
                "value": "",
            }
            environment["observed"]["CFLAGS"] = ""
            write_json(environment_path, environment)
            provenance_path = fixture.profile_root / "provenance.json"
            provenance = json.loads(provenance_path.read_text(encoding="utf-8"))
            provenance["build"]["build_flags"]["CFLAGS"] = ""
            write_json(provenance_path, provenance)
            fixture.resign_all()
            artifact = load_php_src_runtime_artifact(
                fixture.root,
                PROFILE,
                self.contract,
                ELEPHC_SOURCE_COMMIT,
            )

        build_configuration = dict(artifact.provenance.build_configuration)
        self.assertIn('"CFLAGS":""', build_configuration["build_flags"])

    def test_unexpected_observed_build_environment_is_rejected(self) -> None:
        """Re-signing an extra observed environment variable does not admit it."""

        with tempfile.TemporaryDirectory(prefix="elephc-php-src-artifact-") as tmp:
            fixture = PhpSrcArtifactFixture(Path(tmp), self.contract)
            environment_path = fixture.root / "build-environment.json"
            environment = json.loads(environment_path.read_text(encoding="utf-8"))
            environment["observed"]["SECRET_TOKEN"] = "not-allowed"
            write_json(environment_path, environment)
            fixture.resign_all()
            with self.assertRaisesRegex(ContractError, "exceed the allowlist"):
                load_php_src_runtime_artifact(
                    fixture.root,
                    PROFILE,
                    self.contract,
                    ELEPHC_SOURCE_COMMIT,
                )

    def test_noncanonical_configure_args_are_rejected_when_rehashed(self) -> None:
        """An internally coherent alternate PHP build configuration is invalid."""

        with tempfile.TemporaryDirectory(prefix="elephc-php-src-artifact-") as tmp:
            fixture = PhpSrcArtifactFixture(Path(tmp), self.contract)
            provenance_path = fixture.profile_root / "provenance.json"
            provenance = json.loads(provenance_path.read_text(encoding="utf-8"))
            provenance["build"]["configure_args"].append("--enable-fpm")
            provenance["build"]["configure_command"].append("--enable-fpm")
            write_json(provenance_path, provenance)
            fixture.resign_all()
            with self.assertRaisesRegex(ContractError, "configuration is invalid"):
                load_php_src_runtime_artifact(
                    fixture.root,
                    PROFILE,
                    self.contract,
                    ELEPHC_SOURCE_COMMIT,
                )

    def test_unresolved_dynamic_dependency_is_rejected_when_rehashed(self) -> None:
        """A validly rehashed `not found` dependency remains unacceptable."""

        with tempfile.TemporaryDirectory(prefix="elephc-php-src-artifact-") as tmp:
            fixture = PhpSrcArtifactFixture(Path(tmp), self.contract)
            dependency_path = fixture.profile_root / "dynamic-dependencies.txt"
            lines = dependency_path.read_text(encoding="utf-8").splitlines()
            dependency_path.write_text(
                "\n".join([*lines[:4], "libmissing.so => not found"]) + "\n",
                encoding="utf-8",
            )
            fixture.resign_all()
            with self.assertRaisesRegex(ContractError, "unresolved state"):
                load_php_src_runtime_artifact(
                    fixture.root,
                    PROFILE,
                    self.contract,
                    ELEPHC_SOURCE_COMMIT,
                )

    def test_wrong_php_src_commit_is_rejected_even_when_rehashed(self) -> None:
        """A re-signed profile cannot substitute a different php-src commit."""

        with tempfile.TemporaryDirectory(prefix="elephc-php-src-artifact-") as tmp:
            fixture = PhpSrcArtifactFixture(Path(tmp), self.contract)
            provenance_path = fixture.profile_root / "provenance.json"
            document = json.loads(provenance_path.read_text(encoding="utf-8"))
            document["source"]["tag_commit"] = "b" * 40
            write_json(provenance_path, document)
            fixture.resign_profile_and_root()
            with self.assertRaisesRegex(ContractError, "pin or checkout"):
                load_php_src_runtime_artifact(
                    fixture.root,
                    PROFILE,
                    self.contract,
                    ELEPHC_SOURCE_COMMIT,
                )

    def test_install_tree_mode_tampering_is_rejected(self) -> None:
        """Changing an installed file mode invalidates the frozen tree."""

        with tempfile.TemporaryDirectory(prefix="elephc-php-src-artifact-") as tmp:
            fixture = PhpSrcArtifactFixture(Path(tmp), self.contract)
            data = fixture.profile_root / "install" / "lib" / "php" / "core.dat"
            data.chmod(0o600)
            with self.assertRaises(ContractError):
                load_php_src_runtime_artifact(
                    fixture.root,
                    PROFILE,
                    self.contract,
                    ELEPHC_SOURCE_COMMIT,
                )

    def test_runtime_probe_drift_is_rejected(self) -> None:
        """Post-publication extension drift is rejected despite intact files."""

        with tempfile.TemporaryDirectory(prefix="elephc-php-src-artifact-") as tmp:
            fixture = PhpSrcArtifactFixture(Path(tmp), self.contract)
            observed = runtime_probe(fixture.pin.tag.removeprefix("php-"))
            observed["extensions"] = ["Core"]
            with patch(
                "scripts.wasm_oracle.php_src_artifact._probe_php_runtime",
                return_value=observed,
            ):
                with self.assertRaisesRegex(ContractError, "extensions changed"):
                    load_php_src_runtime_artifact(
                        fixture.root,
                        PROFILE,
                        self.contract,
                        ELEPHC_SOURCE_COMMIT,
                    )

    def test_duplicate_hash_manifest_path_is_rejected(self) -> None:
        """Duplicate paths cannot make a later hash silently win."""

        with tempfile.TemporaryDirectory(prefix="elephc-php-src-artifact-") as tmp:
            fixture = PhpSrcArtifactFixture(Path(tmp), self.contract)
            manifest = fixture.root / "hashes.sha256"
            first = manifest.read_text(encoding="utf-8").splitlines()[0]
            manifest.write_text(
                manifest.read_text(encoding="utf-8") + first + "\n",
                encoding="utf-8",
            )
            with self.assertRaises(ContractError):
                load_php_src_runtime_artifact(
                    fixture.root,
                    PROFILE,
                    self.contract,
                    ELEPHC_SOURCE_COMMIT,
                )

    def test_symlinked_build_root_is_rejected(self) -> None:
        """The published build-set root itself must not be replaceable by a symlink."""

        with tempfile.TemporaryDirectory(prefix="elephc-php-src-artifact-") as tmp:
            temporary_root = Path(tmp)
            fixture = PhpSrcArtifactFixture(temporary_root / "build", self.contract)
            alias = temporary_root / "build-alias"
            alias.symlink_to(fixture.root, target_is_directory=True)
            with self.assertRaisesRegex(ContractError, "real directory"):
                load_php_src_runtime_artifact(
                    alias,
                    PROFILE,
                    self.contract,
                    ELEPHC_SOURCE_COMMIT,
                )

    def test_invalid_expected_elephc_commit_is_rejected(self) -> None:
        """The external exact-revision anchor must itself be canonical."""

        with tempfile.TemporaryDirectory(prefix="elephc-php-src-artifact-") as tmp:
            fixture = PhpSrcArtifactFixture(Path(tmp), self.contract)
            with self.assertRaisesRegex(ContractError, "40 lowercase"):
                load_php_src_runtime_artifact(
                    fixture.root,
                    PROFILE,
                    self.contract,
                    "not-a-commit",
                )


if __name__ == "__main__":
    unittest.main()
