#!/usr/bin/env python3
"""Checkpoint main history and request bounded, idempotent development backfills."""

import argparse
import base64
import datetime as dt
import json
import os
from pathlib import Path
import re
import subprocess
import time

BRANCH = "ci/development-artifact-ledger"
FILE = "development-artifacts.json"
WORKFLOW = "development-builds.yml"
TARGETS = {"linux-x86_64", "linux-aarch64", "macos-aarch64"}
PAGE = 20
MAX_PENDING = 100
MAX_DISPATCH = 3
MAX_ATTEMPTS = 3


def git(*args):
    """Read local main history without changing the checkout."""
    return subprocess.check_output(["git", *args], text=True).strip()


def rollout():
    """Start at the first main commit introducing this workflow, never before it."""
    commits = git("log", "--first-parent", "--diff-filter=A", "--format=%H",
                  "origin/main", "--", f".github/workflows/{WORKFLOW}").splitlines()
    if not commits:
        raise ValueError("Development artifact rollout is not on main yet")
    return commits[-1]


def validate_source(sha):
    """Accept full SHAs on the post-rollout first-parent chain only."""
    if not re.fullmatch(r"[0-9a-f]{40}", sha):
        raise ValueError("source-sha must be a full lowercase commit SHA")
    anchor = rollout()
    history = git("rev-list", "--first-parent", f"{anchor}..origin/main").splitlines()
    if sha != anchor and sha not in history:
        raise ValueError("source-sha is not a retained main commit after rollout")
    return sha


def gh(repo, endpoint, method="GET", body=None):
    """Use the official CLI for all repository reads, ledger writes and dispatches."""
    command = ["gh", "api", f"repos/{repo}/{endpoint}", "--method", method]
    if body is not None:
        command += ["--input", "-"]
    result = subprocess.run(command, input=json.dumps(body) if body is not None else None,
                            text=True, capture_output=True, check=True)
    return json.loads(result.stdout) if result.stdout.strip() else None


def initial_ledger(anchor, now):
    """Include rollout itself, while excluding all earlier repository history."""
    return {"schema": 1, "checkpoint": anchor, "pending": {
        anchor: {"first_seen": now, "attempts": 0}}, "completed": []}


def record_page(ledger, commits, now):
    """Record every request before advancing the checkpoint across that commit."""
    for sha in commits[:min(PAGE, MAX_PENDING - len(ledger["pending"]))]:
        ledger["pending"].setdefault(sha, {"first_seen": now, "attempts": 0})
        ledger["checkpoint"] = sha


def produced(sha, artifacts):
    """Expired packages still prove production and must never trigger a rebuild loop."""
    targets = set()
    for artifact in artifacts:
        for target in TARGETS:
            if re.fullmatch(rf"elephc-dev-{target}-{sha}-[1-9][0-9]*", artifact["name"]):
                if artifact.get("size_in_bytes", 0) > 0:
                    targets.add(target)
    return targets == TARGETS


def dispatch_due(record, now, running_since=None):
    """Bound retries and give existing CI or an unobserved dispatch time to finish."""
    if record["attempts"] >= MAX_ATTEMPTS:
        return False
    if now - record.get("last_dispatch", 0) < 600:
        return False
    return running_since is None or now - running_since >= 21600


class Reconciler:
    """Serialize ledger updates and recover dispatches across runner interruption."""

    def __init__(self, repo, now=None):
        self.repo = repo
        self.now = int(time.time()) if now is None else now
        self.file_sha = None

    def api(self, endpoint, method="GET", body=None):
        """Keep the transport replaceable in failure-path tests."""
        return gh(self.repo, endpoint, method, body)

    def load(self):
        """Only initialize on a confirmed missing file; propagate other API errors."""
        try:
            result = self.api(f"contents/{FILE}?ref={BRANCH}")
        except subprocess.CalledProcessError as error:
            if "HTTP 404" not in error.stderr:
                raise
            try:
                self.api(f"git/ref/heads/{BRANCH}")
            except subprocess.CalledProcessError as branch_error:
                if "HTTP 404" not in branch_error.stderr:
                    raise
                self.api("git/refs", "POST", {"ref": f"refs/heads/{BRANCH}",
                                             "sha": git("rev-parse", "origin/main")})
            return initial_ledger(rollout(), self.now)
        self.file_sha = result["sha"]
        ledger = json.loads(base64.b64decode(result["content"]))
        if ledger.get("schema") != 1:
            raise ValueError("Unknown development ledger schema")
        return ledger

    def save(self, ledger):
        """Use Contents API compare-and-swap to reject a concurrent ledger writer."""
        body = {"branch": BRANCH, "message": "ci: checkpoint development artifacts",
                "content": base64.b64encode((json.dumps(ledger, indent=2) + "\n").encode()).decode()}
        if self.file_sha:
            body["sha"] = self.file_sha
        result = self.api(f"contents/{FILE}", "PUT", body)
        self.file_sha = result["content"]["sha"]

    def pages(self, endpoint, key, limit=20):
        """Bound API work explicitly, failing closed on truncated artifact inventories."""
        values = []
        separator = "&" if "?" in endpoint else "?"
        for page in range(1, limit + 1):
            data = self.api(f"{endpoint}{separator}per_page=100&page={page}")[key]
            values.extend(data)
            if len(data) < 100:
                return values
        raise ValueError(f"API pagination limit reached: {endpoint}")

    def observe(self, sha, backfills):
        """Match main push runs by SHA, and explicit builds by their exact source title."""
        runs = self.api(f"actions/workflows/ci.yml/runs?event=push&head_sha={sha}&per_page=20")["workflow_runs"]
        runs = [r for r in runs if r["head_sha"] == sha and r["head_branch"] == "main"]
        runs += [r for r in backfills if r["display_title"] == f"Development artifacts {sha}"
                 and r["event"] == "workflow_dispatch" and r["head_branch"] == "main"]
        running = []
        for run in runs:
            artifacts = self.pages(f"actions/runs/{run['id']}/artifacts", "artifacts")
            if produced(sha, artifacts):
                return run["id"], None
            # GitHub can delete expired artifact records. Successful package jobs
            # are durable evidence that those packages existed during this run.
            jobs = self.pages(f"actions/runs/{run['id']}/jobs?filter=all", "jobs")
            packaged = {target for target in TARGETS if any(
                job["name"].endswith(f"Package development build ({target})")
                and job["conclusion"] == "success" for job in jobs)}
            if packaged == TARGETS:
                return run["id"], None
            if run["status"] != "completed":
                running.append(int(dt.datetime.fromisoformat(run["created_at"].replace("Z", "+00:00")).timestamp()))
        return None, max(running) if running else None

    def reconcile(self):
        """Persist the page, then submit a bounded number of explicit requests."""
        ledger = self.load()
        subprocess.run(["git", "merge-base", "--is-ancestor", ledger["checkpoint"],
                        "origin/main"], check=True)
        commits = git("rev-list", "--first-parent", "--reverse",
                      f"{ledger['checkpoint']}..origin/main").splitlines()
        record_page(ledger, commits, self.now)
        self.save(ledger)  # A crash after this point cannot lose this history page.
        # Only the recent dispatch window is needed. Older known requests retain
        # their run IDs below, so API work does not grow with repository age.
        backfills = self.api(f"actions/workflows/{WORKFLOW}/runs?event=workflow_dispatch&per_page=100")["workflow_runs"]
        dispatched = 0
        pending_page = sorted(ledger["pending"].items(), key=lambda item: item[1].get("last_observed", 0))[:PAGE]
        for sha, record in pending_page:
            record["last_observed"] = self.now
            known = list(backfills)
            for run_id in record.get("runs", []):
                if not any(r["id"] == run_id for r in known):
                    known.append(self.api(f"actions/runs/{run_id}"))
            record["runs"] = [r["id"] for r in known if r["display_title"] == f"Development artifacts {sha}"]
            run, running = self.observe(sha, known)
            if run:
                del ledger["pending"][sha]
                ledger["completed"].append({"sha": sha, "run_id": run, "observed_at": self.now})
                ledger["completed"] = ledger["completed"][-50:]
                self.save(ledger)
            elif dispatched < MAX_DISPATCH and dispatch_due(record, self.now, running):
                # Persist intent first. If dispatch response is lost, the next pass
                # discovers the exact run title before considering another retry.
                record["attempts"] += 1
                record["last_dispatch"] = self.now
                self.save(ledger)
                self.api(f"actions/workflows/{WORKFLOW}/dispatches", "POST",
                         {"ref": "main", "inputs": {"source-sha": sha}})
                dispatched += 1
        self.save(ledger)
        pending = ledger["pending"]
        exhausted = [sha for sha, record in pending.items() if record["attempts"] >= MAX_ATTEMPTS]
        summary = (f"Development artifact checkpoint: `{ledger['checkpoint']}`. "
                   f"Pending: {len(pending)}. Dispatched: {dispatched}.\n")
        if pending:
            summary += "\nPending source commits:\n" + "".join(f"- `{sha}`\n" for sha in pending)
        if exhausted:
            summary += "\nRetry budget exhausted; inspect and manually dispatch these commits: " + ", ".join(exhausted) + "\n"
        print(summary)
        if os.environ.get("GITHUB_STEP_SUMMARY"):
            with Path(os.environ["GITHUB_STEP_SUMMARY"]).open("a") as stream:
                stream.write(summary)
        if exhausted:
            raise SystemExit(1)


def main():
    """Expose source validation and the scheduled reconciler separately."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("validate", "reconcile"))
    parser.add_argument("sha", nargs="?")
    args = parser.parse_args()
    if args.command == "validate":
        print(f"sha={validate_source(args.sha or '')}")
    else:
        Reconciler(os.environ["GITHUB_REPOSITORY"]).reconcile()


if __name__ == "__main__":
    main()
