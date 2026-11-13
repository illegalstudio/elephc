#!/usr/bin/env bash
# Cancel the earlier merge_group runs of the same pull request once the queue rebuilt its group.
#
# The merge queue rebuilds an entry's merge group whenever the base branch moves or an entry
# ahead of it leaves the queue, and every rebuild starts a fresh merge_group run on a new
# `gh-readonly-queue/<base>/pr-<number>-<base sha>` ref. Nothing stops the run of the group
# that was just replaced: its ref differs, so the per-ref concurrency group never matches, and
# `cancel_superseded_queue_runs.sh` only runs when a commit lands. The old run keeps holding
# runners (the macOS ones above all) for a commit the queue will never merge, and the live run
# of the same entry waits behind it until the queue's check timeout removes the entry.
#
# This script runs at the start of a merge_group run and cancels the runs of this workflow
# that test an EARLIER group of the same pull request: same `pr-<number>-` prefix, lower run id.
# A newer run of the same entry is left alone (its own first job cancels this run). Any API
# failure cancels FEWER runs, never more, and only warns: this job never fails a merge group.
#
# Environment: GH_TOKEN, REPO (owner/name), WORKFLOW_FILE (this workflow's file name, e.g.
# ci.yml), HEAD_REF (`github.event.merge_group.head_ref`), RUN_ID (`github.run_id`), and
# optionally GITHUB_STEP_SUMMARY.
set -euo pipefail

: "${REPO:?}" "${WORKFLOW_FILE:?}" "${HEAD_REF:?}" "${RUN_ID:?}"

branch=${HEAD_REF#refs/heads/}
if [[ $branch != gh-readonly-queue/*/pr-*-* ]]; then
    echo "::warning::$HEAD_REF is not a merge queue ref, so no run is cancelled"
    exit 0
fi
# Drop the `-<base sha>` suffix: the prefix names the entry, the suffix names one of its groups.
entry=${branch%-*}

# The statuses of a run that has not completed, listed one at a time and in the order a run
# goes through them; see cancel_superseded_queue_runs.sh for why.
active_states="requested pending queued waiting in_progress"

runs=""
for state in $active_states; do
    if ! listed=$(gh api --paginate "repos/$REPO/actions/workflows/$WORKFLOW_FILE/runs?event=merge_group&status=$state&per_page=100" \
        --jq '.workflow_runs[] | "\(.id) \(.head_branch)"'); then
        echo "::warning::could not list the $state merge_group runs of $WORKFLOW_FILE, so none of them is cancelled"
        continue
    fi
    runs+="$listed"$'\n'
done

cancelled=0
seen=" "
while read -r run_id run_branch; do
    [[ -n $run_id && $seen != *" $run_id "* ]] || continue
    seen+="$run_id "
    [[ $run_branch == "$entry-"* ]] || continue
    if (( run_id >= RUN_ID )); then
        echo "$WORKFLOW_FILE run $run_id ($run_branch) is this run or a newer group of the same entry, so it keeps running"
        continue
    fi
    # A run that completes between the listing and this call answers 409, which is harmless.
    if gh api --method POST "repos/$REPO/actions/runs/$run_id/cancel" > /dev/null; then
        echo "cancelled $WORKFLOW_FILE run $run_id: $run_branch was rebuilt as $branch"
        cancelled=$((cancelled + 1))
    else
        echo "::warning::could not cancel $WORKFLOW_FILE run $run_id ($run_branch)"
    fi
done <<< "$runs"

echo "cancelled $cancelled rebuilt merge_group run(s) of $entry"
if [[ -n ${GITHUB_STEP_SUMMARY:-} ]]; then
    echo "**Merge queue cleanup:** cancelled $cancelled earlier merge_group run(s) of $entry replaced by $branch" >> "$GITHUB_STEP_SUMMARY"
fi
