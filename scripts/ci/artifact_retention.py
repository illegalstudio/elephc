"""Keep the latest complete main packages and delete finished CI intermediates through gh."""

import argparse
import base64
import datetime as dt
import json
import os
from pathlib import Path
import subprocess
import sys
import time
from urllib.parse import quote

from artifact_retention_policy import (CI, DEVELOPMENT, PACKAGE, RETENTION, active_source, candidate,
                                       keep_ids, newest, refresh_due, trusted_run)
from reconcile_development import BRANCH, git
from artifact_retention_api import RetentionAPI, run_safely

FILE = "artifact-retention.json"
MAX_RUNS = 20
MAX_DELETIONS = 500


def missing(error):
    """Only a confirmed 404 means an object is absent."""
    return isinstance(error, subprocess.CalledProcessError) and "HTTP 404" in (error.stderr or "")


def iso(seconds):
    """Use a stable UTC representation for the discovery cursor."""
    return dt.datetime.fromtimestamp(seconds, dt.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


class Retention:
    """Serialize a durable selection before deleting anything it supersedes."""

    def __init__(self, repo, apply=False, now=None):
        self.repo = repo
        self.apply = apply
        self.now = int(time.time()) if now is None else now
        self.file_sha = None
        self.branch_exists = False
        self.summary = []
        self.transport = RetentionAPI(repo)

    def api(self, endpoint, method="GET", body=None):
        """Route every GitHub read and write through the official CLI."""
        return self.transport.request(endpoint, method, body)

    def request_budget(self):
        """Dry runs plan the normal batch; writes respect the token's live quota."""
        return self.transport.budget() if self.apply else MAX_DELETIONS

    def pages(self, endpoint, key, limit=20):
        """Never interpret a truncated discovery or artifact inventory as complete."""
        values = []
        for page in range(1, limit + 1):
            separator = "&" if "?" in endpoint else "?"
            response = self.api(f"{endpoint}{separator}per_page=100&page={page}")
            if key == "workflow_runs" and response.get("total_count", 0) > 1000:
                raise ValueError("Workflow discovery exceeds GitHub's 1000-result search limit")
            data = response[key]
            values.extend(data)
            if len(data) < 100:
                return values
        raise ValueError(f"Refusing cleanup after a truncated inventory: {endpoint}")

    def load(self):
        """Bootstrap only after this policy lands on main; dry-runs never create a branch."""
        try:
            value = self.api(f"contents/{FILE}?ref={BRANCH}")
        except subprocess.CalledProcessError as error:
            if not missing(error):
                raise
            introduced = git("log", "--first-parent", "--diff-filter=A", "--format=%cI",
                             "origin/main", "--", RETENTION).splitlines()
            if self.apply and not introduced:
                raise ValueError("Artifact retention has not rolled out on main")
            return {"schema": 1, "since": introduced[-1] if introduced else iso(self.now),
                    "pending": [], "queues": [], "latest": None}
        self.file_sha = value["sha"]
        self.branch_exists = True
        state = json.loads(base64.b64decode(value["content"]))
        if state.get("schema") != 1:
            raise ValueError("Unknown artifact retention ledger schema")
        return state

    def save(self, state):
        """Persist before deletions so interruption never loses the protected download IDs."""
        if not self.apply:
            return
        if not self.branch_exists:
            try:
                self.api(f"git/ref/heads/{BRANCH}")
            except subprocess.CalledProcessError as error:
                if not missing(error):
                    raise
                self.api("git/refs", "POST", {"ref": f"refs/heads/{BRANCH}",
                                             "sha": git("rev-parse", "origin/main")})
            self.branch_exists = True
        body = {"branch": BRANCH, "message": "ci: checkpoint artifact retention",
                "content": base64.b64encode((json.dumps(state, indent=2) + "\n").encode()).decode()}
        if self.file_sha:
            body["sha"] = self.file_sha
        result = self.api(f"contents/{FILE}", "PUT", body)
        self.file_sha = result["content"]["sha"]

    def discover(self, state, run_id):
        """Record completed-event IDs and checkpoint scheduled discovery before acting."""
        pending = dict.fromkeys(state["pending"] + state["queues"])
        if state["latest"]:
            pending[state["latest"]["run_id"]] = None
        if run_id:
            pending[run_id] = None
        else:
            runs = self.pages(f"actions/runs?created={quote('>=' + state['since'], safe='')}", "workflow_runs")
            pending.update(dict.fromkeys(sorted(run["id"] for run in runs if trusted_run(run, self.repo))))
            state["since"] = iso(self.now - 60)
        # This workflow can later upload refreshed packages after the janitor job
        # finishes. Its creation predates the cursor, so explicitly revisit it.
        if os.environ.get("GITHUB_WORKFLOW") == "Artifact retention":
            pending[int(os.environ["GITHUB_RUN_ID"])] = None
        state["pending"] = list(pending)
        self.save(state)

    def active_runs(self):
        """Protect consumers with no uploaded artifacts yet, including waiting main runs."""
        result = {}
        for status in ("queued", "in_progress", "waiting", "pending", "requested"):
            for run in self.pages(f"actions/runs?status={status}", "workflow_runs"):
                if trusted_run(run, self.repo):
                    result[run["id"]] = run
        return result

    def queue_exists(self, run):
        """An abandoned queue can release its payload after a short merge propagation grace."""
        branch = run.get("head_branch", "")
        if not branch.startswith("gh-readonly-queue/main/"):
            return False
        try:
            self.api("git/ref/heads/" + quote(branch, safe="/"))
            return True
        except subprocess.CalledProcessError as error:
            if not missing(error):
                raise
            return False

    def remove(self, artifact):
        """Delete only an exact inventoried ID, treating an already removed object idempotently."""
        if self.apply:
            try:
                self.api(f"actions/artifacts/{artifact['id']}", "DELETE")
            except subprocess.CalledProcessError as error:
                if not missing(error):
                    raise

    def preserve_benchmarks(self, run, artifacts):
        """Commit measurements to the ledger before deleting the daily importer's inputs."""
        benchmarks = [artifact for artifact in artifacts if artifact["name"] == "benchmark-results"]
        if not benchmarks:
            return
        sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "benchmark"))
        from archive import preserve
        for artifact in benchmarks:
            preserve(self.repo, run, artifact, self.api, self.apply)

    def run(self, run_id=None):
        """Select first, persist second, then dispose of completed runs with fresh status checks."""
        self.request_budget()
        history = git("rev-list", "--first-parent", "origin/main").splitlines()
        state = self.load()
        self.discover(state, run_id)
        selected = set(state["pending"][:MAX_RUNS])
        if state["latest"]:
            selected.add(state["latest"]["run_id"])
        deferred = [identifier for identifier in state["pending"] if identifier not in selected]
        records, candidates = {}, []
        for identifier in state["pending"]:
            if identifier not in selected:
                continue
            try:
                run = self.api(f"actions/runs/{identifier}")
            except subprocess.CalledProcessError as error:
                if (missing(error) and (not state["latest"]
                                       or identifier != state["latest"]["run_id"])):
                    continue
                raise
            if not trusted_run(run, self.repo):
                continue
            if run["status"] != "completed":
                records[identifier] = (run, [], [])
                continue
            artifacts = self.pages(f"actions/runs/{identifier}/artifacts", "artifacts")
            needs_jobs = artifacts and run.get("conclusion") == "success" and (
                run.get("event") == "merge_group" or (
                    run.get("head_branch") == "main" and run.get("head_sha") in history
                    and run.get("path") in (CI, DEVELOPMENT, RETENTION)
                    and any(PACKAGE.fullmatch(a["name"]) for a in artifacts)))
            jobs = self.pages(f"actions/runs/{identifier}/jobs?filter=latest", "jobs") if needs_jobs else []
            records[identifier] = (run, jobs, artifacts)
            value = candidate(run, jobs, artifacts, self.repo, history)
            if value:
                candidates.append(value)
        previous = state["latest"]
        state["latest"] = newest(previous, candidates, history)
        latest = state["latest"]
        if latest:
            run, _, artifacts = records[latest["run_id"]]
            if run["status"] == "completed":
                available = {a["id"] for a in artifacts if not a.get("expired", True)}
                if any(a["id"] not in available for a in latest["artifacts"].values()):
                    raise ValueError("Latest download is missing or expired; rerun development-builds.yml "
                                     "for its source SHA before cleanup can continue")
        self.save(state)
        active = self.active_runs()
        sources = {value for run in active.values() if (value := active_source(run))}
        pending, queues = list(deferred), []
        remaining = min(MAX_DELETIONS, self.request_budget())
        delete_limit = remaining
        partial = False
        for identifier, (run, jobs, artifacts) in records.items():
            # A user can request a rerun after discovery. Never delete inputs from
            # a newer attempt or a run that has become active in the meantime.
            current = self.api(f"actions/runs/{identifier}")
            if (run["status"] != "completed" or current["status"] != "completed"
                    or current["run_attempt"] != run["run_attempt"]):
                pending.append(identifier)
                continue
            if artifacts and run.get("event") == "merge_group":
                sources.update(value for reader in self.active_runs().values()
                               if (value := active_source(reader)))
            exists = self.queue_exists(run) if artifacts and run.get("event") == "merge_group" else False
            keep = keep_ids(run, jobs, artifacts, state["latest"], self.repo, history,
                            sources, exists, self.now)
            deleted = [a for a in artifacts if a["id"] not in keep]
            if len(deleted) > remaining:
                pending.append(identifier)
                partial = True
            deleted = deleted[:remaining]
            self.preserve_benchmarks(run, deleted)
            for artifact in deleted:
                self.remove(artifact)
            remaining -= len(deleted)
            if run.get("event") == "merge_group" and keep:
                queues.append(identifier)
            if run.get("head_branch") == "main" and run.get("head_sha") not in history:
                pending.append(identifier)
            self.summary.append({"run_id": identifier, "delete_count": len(deleted),
                                 "delete_bytes": sum(a["size_in_bytes"] for a in deleted),
                                 "keep_count": sum(a["id"] in keep for a in artifacts)})
        state["pending"], state["queues"] = list(dict.fromkeys(pending)), queues
        self.save(state)
        latest = state["latest"]
        # Every continuation creates another janitor run. Merely forgetting that
        # empty run must not keep dispatching while real workflows are still active.
        janitors = {identifier for identifier, (run, _, _) in records.items() if run["path"] == RETENTION}
        finished = selected - set(pending) - set(queues) - janitors
        if latest:
            finished.discard(latest["run_id"])
        result = {"apply": self.apply, "latest_sha": latest["sha"] if latest else None,
                  "downloads": {target: f"https://github.com/{self.repo}/actions/runs/"
                                f"{latest['run_id']}/artifacts/{artifact['id']}"
                                for target, artifact in latest["artifacts"].items()} if latest else {},
                  "runs": self.summary, "refresh": [],
                  # Continue only after measurable progress, never spin on active readers.
                  "continue_cleanup": bool(deferred or partial) and (
                      remaining < delete_limit or bool(finished))}
        if refresh_due(latest, self.now):
            result["refresh"] = [dict(target=target, sha=latest["sha"], **{
                key: artifact[key] for key in ("id", "name", "digest")})
                for target, artifact in sorted(latest["artifacts"].items())]
        return result


def main():
    """Default to a read-only plan; writes require the workflow's explicit --apply flag."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--run-id", type=int)
    parser.add_argument("--apply", action="store_true")
    args = parser.parse_args()
    result = run_safely(Retention(os.environ["GITHUB_REPOSITORY"], args.apply), args.run_id)
    print(json.dumps(result, indent=2))
    if os.environ.get("GITHUB_OUTPUT"):
        with Path(os.environ["GITHUB_OUTPUT"]).open("a") as output:
            output.write("refresh=" + json.dumps(result["refresh"]) + "\n")
            output.write("continue_cleanup=" + json.dumps(result["continue_cleanup"]) + "\n")
    if os.environ.get("GITHUB_STEP_SUMMARY"):
        with Path(os.environ["GITHUB_STEP_SUMMARY"]).open("a") as output:
            if result.get("deferred"):
                output.write("Artifact retention deferred: " + result["deferred"] + ".\n\n")
            else:
                output.write("Artifact retention: " + ("applied" if args.apply else "dry run") + ".\n\n")
                output.write(f"Latest complete main source: `{result['latest_sha']}`.\n\n")
            for target, url in sorted(result["downloads"].items()):
                output.write(f"- [{target}]({url})\n")
            output.write("\n")
            for row in result["runs"]:
                output.write(f"- Run `{row['run_id']}`: remove {row['delete_count']} artifacts "
                             f"({row['delete_bytes']} bytes), keep {row['keep_count']}.\n")


if __name__ == "__main__":
    main()
