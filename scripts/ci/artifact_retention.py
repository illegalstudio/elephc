"""Keep the latest complete main packages and delete finished CI intermediates through gh."""

import argparse
import base64
import datetime as dt
import json
import os
from pathlib import Path
import subprocess
import time
from urllib.parse import quote

from artifact_retention_policy import (RETENTION, active_source, candidate,
                                       keep_ids, newest, refresh_due, trusted_run)
from github_jobs import read_jobs
from reconcile_development import BRANCH, gh, git

FILE = "artifact-retention.json"


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

    def api(self, endpoint, method="GET", body=None):
        """Route every GitHub read and write through the official CLI."""
        return gh(self.repo, endpoint, method, body)

    def pages(self, endpoint, key, limit=20):
        """Never interpret a truncated discovery or artifact inventory as complete."""
        if key == "jobs":
            return read_jobs(self.api, endpoint, max_items=limit * 100)
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
        pending = set(state["pending"]) | set(state["queues"])
        if state["latest"]:
            pending.add(state["latest"]["run_id"])
        if run_id:
            pending.add(run_id)
        else:
            runs = self.pages(f"actions/runs?created={quote('>=' + state['since'], safe='')}", "workflow_runs")
            pending.update(run["id"] for run in runs if trusted_run(run, self.repo))
            state["since"] = iso(self.now - 60)
        # This workflow can later upload refreshed packages after the janitor job
        # finishes. Its creation predates the cursor, so explicitly revisit it.
        if os.environ.get("GITHUB_WORKFLOW") == "Artifact retention":
            pending.add(int(os.environ["GITHUB_RUN_ID"]))
        state["pending"] = sorted(pending)
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

    def run(self, run_id=None):
        """Select first, persist second, then dispose of completed runs with fresh status checks."""
        history = git("rev-list", "--first-parent", "origin/main").splitlines()
        state = self.load()
        self.discover(state, run_id)
        records, candidates = {}, []
        for identifier in state["pending"]:
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
            jobs = self.pages(f"actions/runs/{identifier}/jobs?filter=latest", "jobs") if artifacts else []
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
        pending, queues = [], []
        for identifier, (run, jobs, artifacts) in records.items():
            # A user can request a rerun after discovery. Never delete inputs from
            # a newer attempt or a run that has become active in the meantime.
            current = self.api(f"actions/runs/{identifier}")
            if (run["status"] != "completed" or current["status"] != "completed"
                    or current["run_attempt"] != run["run_attempt"]):
                pending.append(identifier)
                continue
            if run.get("event") == "merge_group":
                sources.update(value for reader in self.active_runs().values()
                               if (value := active_source(reader)))
            exists = self.queue_exists(run) if run.get("event") == "merge_group" else False
            keep = keep_ids(run, jobs, artifacts, state["latest"], self.repo, history,
                            sources, exists, self.now)
            deleted = [a for a in artifacts if a["id"] not in keep]
            for artifact in deleted:
                self.remove(artifact)
            if run.get("event") == "merge_group" and keep:
                queues.append(identifier)
            if run.get("head_branch") == "main" and run.get("head_sha") not in history:
                pending.append(identifier)
            self.summary.append({"run_id": identifier, "delete_count": len(deleted),
                                 "delete_bytes": sum(a["size_in_bytes"] for a in deleted),
                                 "keep_count": sum(a["id"] in keep for a in artifacts)})
        state["pending"], state["queues"] = pending, queues
        self.save(state)
        latest = state["latest"]
        result = {"apply": self.apply, "latest_sha": latest["sha"] if latest else None,
                  "downloads": {target: f"https://github.com/{self.repo}/actions/runs/"
                                f"{latest['run_id']}/artifacts/{artifact['id']}"
                                for target, artifact in latest["artifacts"].items()} if latest else {},
                  "runs": self.summary, "refresh": []}
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
    result = Retention(os.environ["GITHUB_REPOSITORY"], args.apply).run(args.run_id)
    print(json.dumps(result, indent=2))
    if os.environ.get("GITHUB_OUTPUT"):
        with Path(os.environ["GITHUB_OUTPUT"]).open("a") as output:
            output.write("refresh=" + json.dumps(result["refresh"]) + "\n")
    if os.environ.get("GITHUB_STEP_SUMMARY"):
        with Path(os.environ["GITHUB_STEP_SUMMARY"]).open("a") as output:
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
