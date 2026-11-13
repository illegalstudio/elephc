"""Pace janitor mutations and defer checkpointed work when GitHub limits requests."""

import json
import re
import subprocess
import time

from reconcile_development import gh

RESERVE = 200


class RateDeferred(RuntimeError):
    """The next scheduled invocation may resume the durable pending inventory."""


def rate_limited(error):
    """Distinguish actual rate limits from permission failures and server errors."""
    message = getattr(error, "stderr", None) or str(error)
    return bool(re.search(r"HTTP (403|429)\b", message) and "rate limit" in message.lower())


class RetentionAPI:
    """Keep writes serial with GitHub's recommended one-second mutation spacing."""

    def __init__(self, repository):
        self.repository = repository
        self.last_write = None

    def request(self, endpoint, method="GET", body=None):
        mutation = method != "GET"
        if mutation and self.last_write is not None:
            time.sleep(max(0, self.last_write + 1 - time.monotonic()))
        try:
            return gh(self.repository, endpoint, method, body)
        finally:
            if mutation:
                self.last_write = time.monotonic()

    def budget(self):
        """Leave room for status checks, checkpoint writes and other active CI readers."""
        result = subprocess.run(["gh", "api", "rate_limit"], check=True,
                                capture_output=True, text=True, timeout=60)
        remaining = json.loads(result.stdout)["resources"]["core"]["remaining"]
        if type(remaining) is not int or remaining < 0:
            raise ValueError("Invalid GitHub API remaining quota")
        if remaining <= RESERVE:
            raise RateDeferred(f"GitHub API quota has {remaining} requests remaining; "
                               "pending work will resume on a later scheduled invocation")
        return remaining - RESERVE


def run_safely(janitor, run_id=None):
    """Stop on throttling without losing checkpoints or immediately dispatching another run."""
    try:
        result = janitor.run(run_id)
        if janitor.apply and (result["continue_cleanup"] or result["refresh"]):
            try:
                janitor.request_budget()
            except RateDeferred as error:
                result.update(continue_cleanup=False, refresh=[], deferred=str(error))
        return result
    except (subprocess.CalledProcessError, RuntimeError) as error:
        if not isinstance(error, RateDeferred) and not rate_limited(error):
            raise
        return {"apply": janitor.apply, "latest_sha": None, "downloads": {},
                "runs": janitor.summary, "refresh": [], "continue_cleanup": False,
                "deferred": "GitHub API rate limit: checkpointed work is preserved for the next "
                            "scheduled invocation"}
