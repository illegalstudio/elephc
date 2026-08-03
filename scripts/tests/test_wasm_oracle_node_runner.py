#!/usr/bin/env python3
"""Regression tests for the lossless Node WASI oracle adapter."""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
RUNNER = REPO_ROOT / "scripts" / "wasm_oracle" / "node_runner.mjs"
NODE = shutil.which("node")


@unittest.skipIf(NODE is None, "Node.js is required")
class NodeOracleRunnerTests(unittest.TestCase):
    """Checks separation of PHP streams, host status, and module i32 bits."""

    def run_adapter(
        self,
        *,
        status_expression: str = "-1",
        config_transform=None,
        raw_config: str | None = None,
    ) -> tuple[subprocess.CompletedProcess[bytes], bytes]:
        """Run a synthetic npm loader and return process plus control bytes."""

        with tempfile.TemporaryDirectory(prefix="elephc-node-oracle-") as directory:
            root = Path(directory)
            loader = root / "index.mjs"
            loader.write_text(
                "export async function run(options) {\n"
                "  if (JSON.stringify(options.args) !== "
                'JSON.stringify(["oracle.php", "arg"])) throw new Error("args");\n'
                '  if (options.env.LANG !== "C.UTF-8" '
                '|| options.env.LC_ALL !== "C.UTF-8" '
                '|| options.env.TZ !== "UTC") throw new Error("env");\n'
                "  if (JSON.stringify(options.preopens) !== "
                'JSON.stringify({})) throw new Error("preopens");\n'
                '  process.stdout.write("php-stdout\\n");\n'
                f"  return {status_expression};\n"
                "}\n",
                encoding="utf-8",
            )
            config_data = {
                "args": ["arg"],
                "env": {
                    "LANG": "C.UTF-8",
                    "LC_ALL": "C.UTF-8",
                    "TZ": "UTC",
                },
                "preopens": {},
                "program": "oracle.php",
                "schema": "elephc.wasm-oracle.node-run.v1",
            }
            if config_transform is not None:
                config_transform(config_data)
            config = root / "config.json"
            config.write_text(
                raw_config
                if raw_config is not None
                else (
                    json.dumps(
                        config_data,
                        ensure_ascii=False,
                        separators=(",", ":"),
                        sort_keys=True,
                    )
                    + "\n"
                ),
                encoding="utf-8",
            )

            read_fd, write_fd = os.pipe()
            try:
                environment = {
                    "LANG": "C.UTF-8",
                    "LC_ALL": "C.UTF-8",
                    "TZ": "UTC",
                    "ELEPHC_ORACLE_MODULE_STATUS_FD": str(write_fd),
                }
                process = subprocess.Popen(
                    [
                        NODE,
                        "--no-warnings",
                        str(RUNNER),
                        str(loader),
                        str(config),
                    ],
                    cwd=root,
                    env=environment,
                    pass_fds=(write_fd,),
                    stdout=subprocess.PIPE,
                    stderr=subprocess.PIPE,
                )
                os.close(write_fd)
                write_fd = -1
                stdout, stderr = process.communicate(timeout=10)
                status_payload = os.read(read_fd, 64)
                completed = subprocess.CompletedProcess(
                    process.args,
                    process.returncode,
                    stdout,
                    stderr,
                )
                return completed, status_payload
            finally:
                os.close(read_fd)
                if write_fd >= 0:
                    os.close(write_fd)

    def test_preserves_php_stdout_and_reports_full_i32_bits(self) -> None:
        """A signed -1 status becomes full ffffffff plus POSIX status 255."""

        completed, status_payload = self.run_adapter()

        self.assertEqual(completed.returncode, 255)
        self.assertEqual(completed.stdout, b"php-stdout\n")
        self.assertEqual(completed.stderr, b"")
        self.assertEqual(status_payload, b"ffffffff\n")

    def test_reports_unsigned_i32_max_without_rounding(self) -> None:
        """The largest unsigned i32 status retains all 32 bits exactly."""

        completed, status_payload = self.run_adapter(
            status_expression="0xffff_ffff"
        )

        self.assertEqual(completed.returncode, 255)
        self.assertEqual(status_payload, b"ffffffff\n")

    def test_preserves_explicit_extra_guest_environment_entry(self) -> None:
        """W6 fixtures may add explicit guest variables without host inheritance."""

        def add_secret(config: dict[str, object]) -> None:
            environment = config["env"]
            assert isinstance(environment, dict)
            environment["SECRET_TOKEN"] = "must-not-pass"

        completed, status_payload = self.run_adapter(config_transform=add_secret)

        self.assertEqual(completed.returncode, 255)
        self.assertEqual(completed.stdout, b"php-stdout\n")
        self.assertEqual(completed.stderr, b"")
        self.assertEqual(status_payload, b"ffffffff\n")

    def test_rejects_out_of_range_status_without_control_frame(self) -> None:
        """A status outside i32/u32 cannot be truncated into accepted evidence."""

        completed, status_payload = self.run_adapter(
            status_expression="0x1_0000_0000"
        )

        self.assertNotEqual(completed.returncode, 0)
        self.assertEqual(completed.stdout, b"php-stdout\n")
        self.assertEqual(status_payload, b"")
        self.assertIn(b"not representable as i32", completed.stderr)

    def test_rejects_duplicate_json_keys_as_noncanonical(self) -> None:
        """A duplicate control key cannot be collapsed by JSON.parse."""

        duplicate = (
            '{"args":["arg"],'
            '"env":{"LANG":"C.UTF-8","LC_ALL":"C.UTF-8","TZ":"UTC"},'
            '"preopens":{},"program":"oracle.php",'
            '"schema":"elephc.wasm-oracle.node-run.v1",'
            '"schema":"elephc.wasm-oracle.node-run.v1"}\n'
        )
        completed, status_payload = self.run_adapter(raw_config=duplicate)

        self.assertNotEqual(completed.returncode, 0)
        self.assertEqual(completed.stdout, b"")
        self.assertEqual(status_payload, b"")
        self.assertIn(b"canonical single-line JSON", completed.stderr)


if __name__ == "__main__":
    unittest.main()
