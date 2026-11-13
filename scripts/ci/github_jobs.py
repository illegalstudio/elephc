"""Read bounded Actions job inventories despite large-page errors after partial reruns."""

import re
import subprocess
import sys


def inventory(api, path, page_size, max_items):
    """Reject incomplete responses instead of treating a partial job list as proof."""
    jobs = []
    separator = "&" if "?" in path else "?"
    for page in range(1, (max_items + page_size - 1) // page_size + 1):
        response = api(f"{path}{separator}per_page={page_size}&page={page}")
        batch, total = response["jobs"], response["total_count"]
        if type(total) is not int or total < 0 or total > max_items:
            raise ValueError(f"Invalid or oversized workflow job inventory: {path}")
        if not isinstance(batch, list) or len(batch) > page_size:
            raise ValueError(f"Invalid workflow job page: {path}")
        jobs.extend(batch)
        if len(jobs) == total:
            return jobs
        if len(jobs) > total or len(batch) < page_size:
            raise ValueError(f"Incomplete or changing workflow job inventory: {path}")
    raise ValueError(f"Workflow job pagination exceeded {max_items} items: {path}")


def read_jobs(api, path, max_items=2000):
    """Retry one complete inventory with smaller pages only for transient server errors."""
    try:
        return inventory(api, path, 100, max_items)
    except subprocess.CalledProcessError as error:
        status = re.search(r"HTTP (502|503|504)\b", error.stderr or "")
        if not status:
            raise
        print(f"::warning::Workflow job inventory returned {status.group(0)}; "
              "restarting with 20 jobs per page", file=sys.stderr)
    # A failure on a later large page must discard that attempt's partial list.
    # Preserve the caller's exact run and latest/all filter, and keep the same cap.
    return inventory(api, path, 20, max_items)
