#!/usr/bin/env python3
"""Guard required check ownership, producer dependencies and artifact upload layout."""

from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[2]


def jobs(text):
    """Read this repository's top-level job blocks without requiring a YAML dependency."""
    body = text.split("\njobs:\n", 1)[1]
    starts = list(re.finditer(r"^  ([\w-]+):\n", body, re.M))
    return {match[1]: body[match.end():starts[index + 1].start() if index + 1 < len(starts) else len(body)]
            for index, match in enumerate(starts)}


def dependencies(body):
    """Handle scalar, inline-list and block-list needs used by these workflows."""
    match = re.search(r"^    needs:(.*)(?:\n((?:      - [\w-]+\n)+))?", body, re.M)
    if not match:
        return []
    value = match[1].strip()
    if value.startswith("["):
        return [item.strip() for item in value.strip("[]").split(",")]
    if value:
        return [value]
    return re.findall(r"- ([\w-]+)", match[2] or "")


class ArtifactGraphTests(unittest.TestCase):
    """Structural failures must be caught before a queue waits for a nonexistent check."""

    def setUp(self):
        self.workflow = (ROOT / ".github/workflows/ci.yml").read_text()
        self.jobs = jobs(self.workflow)

    def test_required_names_are_unique_top_level_always_gates(self):
        for name in ("Build & Test", "PDO core live suites (PostgreSQL 16 + MySQL 8.4)",
                     "PDO optional-driver live suites"):
            matches = [body for body in self.jobs.values() if f"    name: {name}\n" in body]
            self.assertEqual(len(matches), 1, name)
            self.assertIn("    if: always()\n", matches[0])
            self.assertIn("    runs-on:", matches[0])
        self.assertIn("  merge_group:\n    types: [checks_requested]", self.workflow)

    def test_all_job_dependencies_exist_and_graph_has_no_cycles(self):
        edges = {name: dependencies(body) for name, body in self.jobs.items()}
        done = set()
        def visit(name, active):
            self.assertIn(name, edges)
            self.assertNotIn(name, active, f"dependency cycle at {name}")
            if name in done:
                return
            for dependency in edges[name]:
                visit(dependency, active | {name})
            done.add(name)
        for name in edges:
            visit(name, set())

    def test_standalone_wrappers_no_longer_own_pr_or_queue_events(self):
        for filename in ("pdo-live.yml", "phpext-host-sdk.yml"):
            text = (ROOT / ".github/workflows" / filename).read_text().split("\njobs:", 1)[0]
            self.assertNotIn("  pull_request:", text)
            self.assertNotIn("  merge_group:", text)
            self.assertNotIn("  push:", text)
        cleanup = (ROOT / ".github/workflows/merge-queue-cleanup.yml").read_text()
        self.assertNotIn("ci.yml pdo-live.yml", cleanup)

    def test_child_workflows_cannot_cancel_their_parent(self):
        for path in (ROOT / ".github/workflows").glob("ci-*.yml"):
            if "  workflow_call:" in path.read_text():
                self.assertNotIn("\nconcurrency:", path.read_text(), str(path))

    def test_container_consumers_use_the_coordinators_immutable_image(self):
        for name, body in self.jobs.items():
            if "    container:" in body:
                self.assertIn("plan", dependencies(body), name)
                self.assertIn("image: ${{ needs.plan.outputs.ci-image }}", body, name)
                self.assertIn("ELEPHC_CI_IMAGE: ${{ needs.plan.outputs.ci-image }}", body, name)

    def test_every_ios_link_consumer_restores_its_target_variant(self):
        for name, body in self.jobs.items():
            if 'ELEPHC_CI_PREBUILT_IOS_BRIDGES: "1"' in body:
                self.assertIn("build-ios", dependencies(body), name)
                self.assertIn("target: ${{ matrix.elephc_target }}\n          kind: ios", body, name)
                self.assertIn("fromJSON(needs.build-ios.outputs.artifact-attempts)[matrix.elephc_target]", body, name)

    def test_artifact_consumers_audit_even_swallowed_build_failures(self):
        for path in (ROOT / ".github/workflows").glob("*.yml"):
            if path.name.startswith("ci-build-"):
                continue
            for name, body in jobs(path.read_text()).items():
                if re.search(r"uses: \./\.github/actions/(restore-|download-test-archive)", body):
                    self.assertIn("if: always()\n        uses: ./.github/actions/assert-consumer-builds", body,
                                  f"{path.name}: {name}")

    def test_workspace_cargo_compilation_is_owned_by_producers(self):
        for path in (ROOT / ".github/workflows").glob("*.yml"):
            if path.name.startswith("ci-build-"):
                continue
            for line in path.read_text().splitlines():
                command = line.strip()
                if not command or command.startswith("#"):
                    continue
                self.assertIsNone(re.search(r"(?:^run: |^|[;&] )cargo (?:build|test|run|check|nextest archive)\b", command),
                                  f"{path.name}: {command}")

    def test_nextest_upload_uses_one_directory_on_container_runners(self):
        producer = (ROOT / ".github/workflows/ci-build-host.yml").read_text()
        blocks = re.split(r"(?=      - (?:name:|uses:))", producer)
        for block in blocks:
            if block.startswith("      - name: Upload ") and "nextest-archive-" in block:
                self.assertNotIn("          path: |", block)
                self.assertIn("${{ github.run_attempt }}", block)

    def test_every_nextest_consumer_uses_its_producer_attempt(self):
        for name, body in self.jobs.items():
            for match in re.finditer(r"name: nextest-archive-(?:codegen-)?(macos-aarch64|linux-x86_64|linux-aarch64)\n", body):
                producer = f"build-archive-{match[1]}"
                self.assertIn(producer, dependencies(body), name)
                self.assertIn(f"attempt: ${{{{ needs.{producer}.outputs.artifact-attempt }}}}", body)


if __name__ == "__main__":
    unittest.main()
