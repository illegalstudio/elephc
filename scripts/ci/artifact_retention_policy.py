"""Select the latest complete main download and disposable completed-run artifacts."""

import datetime as dt
import re

from resolve_main_build import GATES, HOSTS, select

PREFIX = ".github/workflows/"
CI = PREFIX + "ci.yml"
DEVELOPMENT = PREFIX + "development-builds.yml"
RETENTION = PREFIX + "artifact-retention.yml"
WORKFLOWS = {CI, DEVELOPMENT, RETENTION, *(PREFIX + name for name in (
    "pdo-live.yml", "phpext-host-sdk.yml", "ios-curl-live.yml", "nightly.yml", "release.yml"))}
PACKAGE = re.compile(r"elephc-dev-(linux-x86_64|linux-aarch64|macos-aarch64)-([0-9a-f]{40})-([1-9][0-9]*)")


def timestamp(value):
    """Use UTC seconds for API timestamps and retention windows."""
    return int(dt.datetime.fromisoformat(value.replace("Z", "+00:00")).timestamp())


def trusted_run(run, repository):
    """Never let a similarly named workflow or foreign repository enter cleanup."""
    return (run.get("repository", {}).get("full_name") == repository
            and run.get("path") in WORKFLOWS and isinstance(run.get("id"), int))


def candidate(run, jobs, artifacts, repository, history):
    """Only a complete successful main publication can replace the previous set."""
    if (not trusted_run(run, repository) or run.get("status") != "completed"
            or run.get("conclusion") != "success" or run.get("head_branch") != "main"
            or run.get("head_sha") not in history):
        return None
    path, event = run["path"], run.get("event")
    if path == CI:
        if event != "push" or any([j["conclusion"] for j in jobs if j["name"] == gate] != ["success"]
                                  for gate in GATES):
            return None
    elif path == DEVELOPMENT:
        if event != "workflow_dispatch":
            return None
    elif path == RETENTION:
        if event not in ("workflow_run", "workflow_dispatch", "schedule"):
            return None
    else:
        return None
    verb = "Refresh development build" if path == RETENTION else "Package development build"
    if any(not any((j["name"] == f"{verb} ({target})" or j["name"].endswith(f" / {verb} ({target})"))
                   and j["conclusion"] == "success" for j in jobs) for target in HOSTS):
        return None
    groups = {}
    for artifact in artifacts:
        match = PACKAGE.fullmatch(artifact["name"])
        if (not match or artifact.get("expired", True) or artifact.get("size_in_bytes", 0) <= 0
                or artifact.get("workflow_run", {}).get("head_sha") != run["head_sha"]):
            continue
        target, sha, attempt = match.groups()
        if (sha not in history or int(attempt) > run.get("run_attempt", 0)
                or (path == CI and sha != run["head_sha"])):
            continue
        if path == DEVELOPMENT and run.get("display_title") != f"Development artifacts {sha}":
            continue
        group = groups.setdefault(sha, {})
        if target not in group or artifact["id"] > group[target]["id"]:
            group[target] = artifact
    complete = [dict(sha=sha, run_id=run["id"], artifacts=group)
                for sha, group in groups.items() if set(group) == set(HOSTS)]
    return min(complete, key=lambda value: history.index(value["sha"])) if complete else None


def newest(previous, candidates, history):
    """First-parent source order wins over late finishes; refreshes win only for the same SHA."""
    choices = ([previous] if previous else []) + list(candidates)
    if any(value["sha"] not in history for value in choices):
        raise ValueError("A retained package source left main first-parent history")
    def order(value):
        return (history.index(value["sha"]),
                -min(timestamp(a["expires_at"]) for a in value["artifacts"].values()),
                -value["run_id"])
    return min(choices, key=order) if choices else None


def active_source(run):
    """Identify a package consumer even before it has uploaded any artifacts."""
    if run.get("status") == "completed" or run.get("head_branch") != "main":
        return None
    if run.get("path") == CI and run.get("event") == "push":
        return run.get("head_sha")
    if run.get("path") == DEVELOPMENT:
        match = re.fullmatch(r"Development artifacts ([0-9a-f]{40})", run.get("display_title", ""))
        return match[1] if match else None
    return None


def keep_ids(run, jobs, artifacts, latest, repository, history, active_sources,
             queue_exists, now):
    """Retain only the selected download or a minimal live queue-to-main handoff."""
    if not trusted_run(run, repository) or run.get("status") != "completed":
        return {a["id"] for a in artifacts}
    if run.get("head_branch") == "main" and run.get("head_sha") not in history:
        # Main may advance after this janitor checked out its trusted history.
        return {a["id"] for a in artifacts}
    keep = ({a["id"] for a in latest["artifacts"].values()}
            if latest and latest["run_id"] == run["id"] else set())
    if run.get("event") == "merge_group" and run.get("head_sha") in active_sources:
        # Main may have resolved the passed required gates while an optional queue
        # job was still running. Its later failure must not remove an active reader's inputs.
        return keep | {a["id"] for a in artifacts if re.fullmatch(
            r"build-(macos-aarch64|linux-x86_64|linux-aarch64)-debug-(compiler|bridges)-default-[1-9][0-9]*",
            a["name"]) or re.fullmatch(r"ci-environment-[1-9][0-9]*", a["name"])}
    if run.get("event") != "merge_group" or run.get("conclusion") != "success":
        return keep
    selected = select(run, jobs, artifacts, repository, run["head_sha"])
    if not selected:
        return keep
    sha = run["head_sha"]
    consumed = (latest and sha in history
                and history.index(latest["sha"]) <= history.index(sha))
    grace = now - timestamp(run["updated_at"]) < 900
    needed = (sha in active_sources or grace or (not consumed and (sha in history or queue_exists)))
    if needed:
        names = {f"build-{target}-debug-{kind}-default-{attempt}"
                 for target, attempt in selected["attempts"].items()
                 for kind in ("compiler", "bridges")}
        keep.update(a["id"] for a in artifacts if a["name"] in names
                    or re.fullmatch(r"ci-environment-[1-9][0-9]*", a["name"]))
    return keep


def refresh_due(latest, now):
    """Refresh verified bytes before GitHub's mandatory expiry, including shorter org caps."""
    if not latest:
        return False
    return any(timestamp(a["expires_at"]) - now <= max(
        21600, (timestamp(a["expires_at"]) - timestamp(a["created_at"])) // 10)
        for a in latest["artifacts"].values())
