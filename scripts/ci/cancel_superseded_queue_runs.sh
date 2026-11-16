#!/usr/bin/env bash
# Cancel the merge queue runs that no longer gate anything, after a push to the base branch.
#
# With the HEADGREEN grouping the queue merges an entry together with every entry ahead of
# it as soon as that entry's merge_group run passes, so the runs of the entries ahead can
# still be going when their commits are already on the base branch. Nothing else stops
# them: each entry has its own `gh-readonly-queue/...` ref, so a per-ref concurrency group
# never matches, and they keep holding runners (the macOS ones above all) that the entries
# still waiting need.
#
# A run is cancelled only when its commit is a strict ancestor of SHA (compare status
# `ahead`). The run of SHA itself stays: the push run's `plan` job reads it to skip the
# tiers the queue already ran. Runs of later entries (`behind`) and of entries rebuilt on
# another base (`diverged`) may still gate a merge, so they stay too. Any API failure
# cancels FEWER runs, never more, and only warns: this job never fails a push.
#
# Environment: GH_TOKEN, REPO (owner/name), SHA (the new head of the base branch),
# WORKFLOW_FILES (space-separated, e.g. "ci.yml pdo-live.yml"), and optionally
# GITHUB_STEP_SUMMARY.
set -euo pipefail

: "${REPO:?}" "${SHA:?}" "${WORKFLOW_FILES:?}"

# The statuses of a run that has not completed. The API filters on them one at a time, and
# listing only those keeps the lookup complete and short: an unfiltered listing is ordered
# newest first and reaches an old run still waiting for a runner only after every newer one.
# They are listed in the order a run goes through them, `in_progress` last, so a run that
# starts while the listing is under way shows up under a later status instead of slipping
# between two requests.
active_states="requested pending queued waiting in_progress"

cancelled=0
for workflow in $WORKFLOW_FILES; do
    # List every status before cancelling anything: a cancellation frees runners, which can
    # start other runs while the remaining statuses are still to be listed.
    runs=""
    for state in $active_states; do
        if ! listed=$(gh api --paginate "repos/$REPO/actions/workflows/$workflow/runs?event=merge_group&status=$state&per_page=100" \
            --jq '.workflow_runs[] | "\(.id) \(.head_sha)"'); then
            echo "::warning::could not list the $state merge_group runs of $workflow, so none of them is cancelled"
            continue
        fi
        runs+="$listed"$'\n'
    done
    # A run that moved on during the listing appears under two statuses; handle it once.
    seen=" "
    while read -r run_id run_sha; do
        [[ -n $run_id && $seen != *" $run_id "* ]] || continue
        seen+="$run_id "
        if ! relation=$(gh api "repos/$REPO/compare/$run_sha...$SHA?per_page=1" --jq '.status'); then
            echo "::warning::could not compare $workflow run $run_id ($run_sha) with $SHA, so it keeps running"
            continue
        fi
        if [[ $relation != ahead ]]; then
            echo "$workflow run $run_id tests $run_sha ($relation relative to $SHA), so it keeps running"
            continue
        fi
        # A run that completes between the listing and this call answers 409, which is harmless.
        if gh api --method POST "repos/$REPO/actions/runs/$run_id/cancel" > /dev/null; then
            echo "cancelled $workflow run $run_id: $run_sha is already part of $SHA"
            cancelled=$((cancelled + 1))
        else
            echo "::warning::could not cancel $workflow run $run_id ($run_sha)"
        fi
    done <<< "$runs"
done

echo "cancelled $cancelled superseded merge_group run(s)"
if [[ -n ${GITHUB_STEP_SUMMARY:-} ]]; then
    echo "**Merge queue cleanup:** cancelled $cancelled merge_group run(s) whose commits $SHA already contains" >> "$GITHUB_STEP_SUMMARY"
fi
