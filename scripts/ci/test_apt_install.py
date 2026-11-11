"""Exercise the real APT wrapper without modifying the host package manager."""

import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
INSTALLER = ROOT / "scripts/ci/apt_install.sh"


class AptProvisioning(unittest.TestCase):
    """Keep index completeness, signature failures and mirror selection observable."""

    def setUp(self):
        """Provide isolated Ubuntu sources and fake package-manager executables."""
        tmp = tempfile.TemporaryDirectory(prefix=".apt-provisioning.", dir=ROOT)
        self.addCleanup(tmp.cleanup)
        self.root = Path(tmp.name)
        self.bin = self.root / "bin"
        self.sources = self.root / "apt sources with spaces"
        self.runner = self.root / "runner temp"
        self.log = self.root / "calls.jsonl"
        for directory in (self.bin, self.sources / "sources.list.d", self.runner):
            directory.mkdir(parents=True)
        self.ubuntu = self.sources / "sources.list.d/ubuntu.sources"
        self.ubuntu.write_text("Types: deb\nURIs: mirror+file:/etc/apt/apt-mirrors.txt\n"
                               "Suites: noble noble-updates noble-security\n"
                               "Components: main universe\nArchitectures: amd64 arm64\n"
                               "Signed-By: /usr/share/keyrings/ubuntu-archive-keyring.gpg\n")
        self.vendor = self.sources / "sources.list.d/microsoft.list"
        self.vendor.write_text("deb [signed-by=/usr/share/keyrings/microsoft-prod.gpg] "
                               "https://packages.microsoft.com/ubuntu/24.04/prod noble main\n")
        self.env = {**os.environ, "PATH": str(self.bin) + os.pathsep + os.environ["PATH"],
                    "RUNNER_TEMP": str(self.runner), "ELEPHC_CI_APT_ETC": str(self.sources),
                    "APT_TEST_LOG": str(self.log), "APT_TEST_ARCH": "amd64", "LC_ALL": "C"}
        self.tool("dpkg", '#!/bin/sh\nprintf "%s\\n" "$APT_TEST_ARCH"\n')
        self.tool("timeout", '#!/bin/sh\nshift 2\nexec "$@"\n')
        self.tool("apt-get", '''#!/usr/bin/env python3
import json,os,sys
from pathlib import Path
args=sys.argv[1:]
phase='update' if 'update' in args else 'install'
parts=next((value.split('=',1)[1] for value in args if value.startswith('Dir::Etc::sourceparts=')),None)
sources={p.name:p.read_text() for p in Path(parts).iterdir()} if parts else {}
main=next((value.split('=',1)[1] for value in args if value.startswith('Dir::Etc::sourcelist=')),None)
entry={'args':args,'phase':phase,'fallback':bool(parts),'sources':sources,'main':Path(main).read_text() if main else ''}
with open(os.environ['APT_TEST_LOG'],'a') as stream:stream.write(json.dumps(entry)+'\\n')
mode=os.environ.get('APT_TEST_MODE','ok')
if mode=='network' and not parts or mode=='persistent-network':
    print("Temporary failure resolving 'azure.archive.ubuntu.com'")
    sys.exit(100 if '--error-on=any' in args or phase=='install' else 0)
if mode=='install-network' and phase=='install' and not parts:
    print("Could not connect to archive.ubuntu.com:80: Connection timed out")
    sys.exit(100)
if mode=='signature':
    print('NO_PUBKEY 1234; Temporary failure resolving mirror.example')
    sys.exit(100)
if mode=='missing' and phase=='install':
    print('E: Unable to locate package requested-package')
    sys.exit(100)
if mode=='checksum' and phase=='install':
    print('Hash Sum mismatch; Connection reset')
    sys.exit(100)
if mode=='update-timeout' and phase=='update' and not parts:sys.exit(124)
if mode=='install-timeout' and phase=='install':sys.exit(124)
''')

    def tool(self, name, source):
        """Make one test-only executable, never invoking real apt or dpkg."""
        path = self.bin / name
        path.write_text(source)
        path.chmod(0o755)

    def run_installer(self, mode="ok", architecture="amd64"):
        """Run production shell control flow and capture every package-manager call."""
        result = subprocess.run(["bash", str(INSTALLER), "--no-install-recommends", "gdb"],
                                env={**self.env, "APT_TEST_MODE": mode, "APT_TEST_ARCH": architecture},
                                capture_output=True, text=True, timeout=30)
        calls = [json.loads(line) for line in self.log.read_text().splitlines()] if self.log.exists() else []
        self.assertEqual(list(self.runner.iterdir()), [], result.stderr)
        return result, calls

    def test_success_requires_complete_indexes_before_install(self):
        result, calls = self.run_installer()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual([call["phase"] for call in calls], ["update", "install"])
        self.assertIn("--error-on=any", calls[0]["args"])
        self.assertIn("--no-install-recommends", calls[1]["args"])
        self.assertTrue(all("APT::Get::AllowUnauthenticated=false" in call["args"] for call in calls))

    def test_first_attempt_has_no_placeholder_for_empty_source_options(self):
        """Bash 3.2 must omit empty arrays without injecting an empty APT argument."""
        result, calls = self.run_installer()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertTrue(calls)
        for call in calls:
            self.assertNotIn("", call["args"])
            self.assertFalse(call["fallback"])

    def test_network_failure_restarts_indexes_on_the_architecture_specific_mirror(self):
        before = self.ubuntu.read_bytes(), self.vendor.read_bytes()
        for arch, mirror in (("amd64", "mirrors.edge.kernel.org/ubuntu"),
                             ("arm64", "mirrors.dotsrc.org/ubuntu-ports")):
            if self.log.exists():
                self.log.unlink()
            result, calls = self.run_installer("network", arch)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertEqual([call["phase"] for call in calls], ["update", "update", "install"])
            fallback = calls[-1]["sources"]
            self.assertIn(mirror, fallback["ubuntu.sources"])
            self.assertNotIn("mirror+file:", fallback["ubuntu.sources"])
            self.assertIn("Signed-By: /usr/share/keyrings/ubuntu-archive-keyring.gpg", fallback["ubuntu.sources"])
            self.assertIn("Components: main universe", fallback["ubuntu.sources"])
            self.assertEqual(fallback["microsoft.list"], self.vendor.read_text())
            self.assertEqual((self.ubuntu.read_bytes(), self.vendor.read_bytes()), before)

    def test_package_download_network_error_also_restarts_complete_indexes(self):
        result, calls = self.run_installer("install-network")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual([call["phase"] for call in calls], ["update", "install", "update", "install"])
        self.assertTrue(calls[-1]["fallback"])

    def test_legacy_sources_and_canonical_deb822_urls_keep_their_signing_keys(self):
        """Exercise the real Ubuntu image URLs as well as hosted mirror-list sources."""
        (self.sources / "sources.list").write_text(
            "deb [arch=amd64 signed-by=/usr/share/keyrings/ubuntu-archive-keyring.gpg] "
            "http://azure.archive.ubuntu.com/ubuntu/ noble main\n")
        self.ubuntu.write_text(self.ubuntu.read_text().replace(
            "mirror+file:/etc/apt/apt-mirrors.txt",
            "http://ports.ubuntu.com/ubuntu-ports/ https://security.ubuntu.com/ubuntu"))
        result, calls = self.run_installer("network", "arm64")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        final = calls[-1]
        self.assertIn("mirrors.dotsrc.org/ubuntu-ports", final["main"])
        self.assertIn("signed-by=/usr/share/keyrings/ubuntu-archive-keyring.gpg", final["main"])
        self.assertEqual(final["sources"]["ubuntu.sources"].count("mirrors.dotsrc.org/ubuntu-ports"), 2)
        self.assertNotIn("ports.ubuntu.com", final["sources"]["ubuntu.sources"])

    def test_persistent_network_failure_is_bounded_to_two_attempts(self):
        result, calls = self.run_installer("persistent-network")
        self.assertEqual(result.returncode, 100)
        self.assertEqual([call["phase"] for call in calls], ["update", "update"])

    def test_signature_checksum_and_missing_package_fail_without_retry(self):
        for mode, expected in (("signature", 1), ("checksum", 2), ("missing", 2)):
            if self.log.exists():
                self.log.unlink()
            result, calls = self.run_installer(mode)
            self.assertEqual(result.returncode, 100)
            self.assertEqual(len(calls), expected)
            self.assertFalse(any(call["fallback"] for call in calls))

    def test_update_timeout_can_recover_but_install_timeout_is_not_assumed_network(self):
        result, calls = self.run_installer("update-timeout")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(len(calls), 3)
        self.log.unlink()
        result, calls = self.run_installer("install-timeout")
        self.assertEqual(result.returncode, 124)
        self.assertEqual(len(calls), 2)

    def test_unsupported_architecture_fails_before_provisioning(self):
        result, calls = self.run_installer(architecture="armhf")
        self.assertEqual(result.returncode, 2)
        self.assertEqual(calls, [])


class AptWiring(unittest.TestCase):
    """Prevent a later CI job from reintroducing unchecked partial APT indexes."""

    def test_all_workflow_package_installs_use_the_shared_wrapper(self):
        for path in (ROOT / ".github/workflows").glob("*.yml"):
            for line in path.read_text().splitlines():
                if not line.lstrip().startswith("#"):
                    self.assertIsNone(re.search(r"\bapt-get (?:update|install)\b", line), (path, line))

    def test_image_contains_gdb_and_rebuilds_when_its_shared_installer_changes(self):
        image = (ROOT / ".github/docker/ci.Dockerfile").read_text()
        workflow = (ROOT / ".github/workflows/ci-image.yml").read_text()
        self.assertIn("COPY --chmod=0755 scripts/ci/apt_install.sh", image)
        self.assertRegex(image, r"(?m)^\s+gdb \\\s*$")
        self.assertEqual(workflow.count("- 'scripts/ci/apt_install.sh'"), 2)
        self.assertIn("          context: .\n", workflow)

    def test_historical_package_builds_provision_before_switching_source(self):
        workflow = (ROOT / ".github/workflows/ci-package-development.yml").read_text()
        self.assertLess(workflow.index("sudo bash scripts/ci/apt_install.sh"),
                        workflow.index("ref: ${{ steps.source.outputs.sha }}"))


if __name__ == "__main__":
    unittest.main()
