#!/usr/bin/env bash
# Decide which CI tiers a workflow run executes, and write them to $GITHUB_OUTPUT:
#
#   run=true|false       the workflow's test jobs run at all
#   extended=true|false  the tier that pull requests skip by default runs too
#                        (the macOS matrix in ci.yml, the live database suites in pdo-live.yml)
#
# Rules, by event:
#   merge_group, schedule, workflow_dispatch  every tier.
#   push                                      nothing when a successful merge_group run of the
#                                             same workflow already tested this exact commit
#                                             (the merge queue fast-forwards the base branch to
#                                             it); every tier otherwise, e.g. a direct push.
#   pull_request                              the default tier, plus the extended tier when the
#                                             PR carries EXTENDED_LABEL or changes a path that
#                                             matches EXTENDED_PATHS.
#
# Labels are read from the API, not from the event payload, so adding the label and re-running
# the workflow is enough. Any API failure selects MORE tiers, never fewer.
#
# Environment: GH_TOKEN, REPO (owner/name), EVENT, SHA, PR_NUMBER (pull_request only),
# WORKFLOW_FILE (e.g. ci.yml), EXTENDED_LABEL (e.g. ci:macos), EXTENDED_PATHS (an ERE matched
# against each changed path), and optionally GITHUB_OUTPUT / GITHUB_STEP_SUMMARY.
set -euo pipefail

: "${REPO:?}" "${EVENT:?}" "${SHA:?}" "${WORKFLOW_FILE:?}" "${EXTENDED_LABEL:?}" "${EXTENDED_PATHS:?}"

run=true
extended=true
reason=""

case "$EVENT" in
    merge_group | schedule | workflow_dispatch)
        reason="$EVENT runs every tier"
        ;;
    push)
        if count=$(gh api "repos/$REPO/actions/workflows/$WORKFLOW_FILE/runs?head_sha=$SHA&event=merge_group&status=success&per_page=1" --jq '.total_count'); then
            if [[ $count =~ ^[0-9]+$ && $count -gt 0 ]]; then
                run=false
                extended=false
                reason="a successful merge_group run of $WORKFLOW_FILE already tested $SHA"
            else
                reason="no merge_group run of $WORKFLOW_FILE tested $SHA, so every tier runs"
            fi
        else
            reason="could not look up merge_group runs for $SHA, so every tier runs"
        fi
        ;;
    pull_request)
        : "${PR_NUMBER:?}"
        extended=false
        if ! labels=$(gh api "repos/$REPO/pulls/$PR_NUMBER" --jq '.labels[].name'); then
            extended=true
            reason="could not read the labels of #$PR_NUMBER, so every tier runs"
        elif grep -qxF -- "$EXTENDED_LABEL" <<< "$labels"; then
            extended=true
            reason="#$PR_NUMBER carries the $EXTENDED_LABEL label"
        elif ! files=$(gh api --paginate "repos/$REPO/pulls/$PR_NUMBER/files?per_page=100" --jq '.[].filename'); then
            extended=true
            reason="could not list the files of #$PR_NUMBER, so every tier runs"
        elif match=$(grep -m1 -E -- "$EXTENDED_PATHS" <<< "$files"); then
            extended=true
            reason="#$PR_NUMBER changes $match"
        else
            reason="pull request tier; add the $EXTENDED_LABEL label and re-run to include the rest"
        fi
        ;;
    *)
        reason="unrecognized event $EVENT, so every tier runs"
        ;;
esac

echo "run=$run extended=$extended: $reason"
if [[ -n ${GITHUB_OUTPUT:-} ]]; then
    {
        echo "run=$run"
        echo "extended=$extended"
    } >> "$GITHUB_OUTPUT"
fi
if [[ -n ${GITHUB_STEP_SUMMARY:-} ]]; then
    echo "**$WORKFLOW_FILE tiers:** run=\`$run\` extended=\`$extended\` ($reason)" >> "$GITHUB_STEP_SUMMARY"
fi
