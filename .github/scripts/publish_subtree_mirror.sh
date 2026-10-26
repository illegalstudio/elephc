#!/usr/bin/env bash
# Publish one directory of this repository to its own read-only mirror repository.
#
# Usage: publish_subtree_mirror.sh --prefix DIR --repository OWNER/NAME
#            [--history preserve|flatten] [--message TEXT] [--remote URL] [--push]
#
# preserve (the default) publishes `git subtree split --prefix=DIR HEAD`: the history of
# DIR, with its original authors and messages. It publishes only when the mirror's main is
# an ancestor of the split, so a commit made directly on the mirror stops the publish
# instead of being overwritten.
#
# flatten adds one commit per publish, authored by MIRROR_BOT_NAME <MIRROR_BOT_EMAIL>, whose
# tree is HEAD:DIR and whose parent is the mirror's main. --message replaces the default
# message, which names the source commit. It publishes only when that main was committed by
# the bot or comes from an earlier preserve publish (an ancestor of the split), so a mirror
# can switch from preserve to flatten while a commit made on the mirror still stops it. The
# opposite switch would need a history rewrite: the preserve ancestor check refuses it.
#
# Nothing is ever force-pushed, and the source is HEAD, so uncommitted files are never
# published. The split needs the full history (actions/checkout with fetch-depth: 0).
#
# Without --push the script only checks DIR locally and prints the commit (preserve) or the
# tree (flatten) it would publish. With --push it needs MIRROR_TOKEN, a token allowed to
# write OWNER/NAME on GitHub, unless --remote names another remote (the tests use a local
# bare repository). The token travels only through environment-only Git configuration,
# never in a URL or a file.
set -euo pipefail

usage() {
    echo "Usage: publish_subtree_mirror.sh --prefix DIR --repository OWNER/NAME" \
        "[--history preserve|flatten] [--message TEXT] [--remote URL] [--push]" >&2
    exit 2
}

prefix="" repository="" history=preserve message="" remote="" push=false
while [[ $# -gt 0 ]]; do
    case "$1" in
        --prefix|--repository|--history|--message|--remote)
            [[ $# -ge 2 ]] || usage
            case "$1" in
                --prefix) prefix=${2%/} ;;
                --repository) repository=$2 ;;
                --history) history=$2 ;;
                --message) message=$2 ;;
                --remote) remote=$2 ;;
            esac
            shift ;;
        --push) push=true ;;
        *) usage ;;
    esac
    shift
done
[[ -n $prefix && -n $repository ]] || usage
[[ $history == preserve || $history == flatten ]] || usage

cd "$(git rev-parse --show-toplevel)"
source_commit=$(git rev-parse --verify HEAD)
if ! tree=$(git rev-parse --verify --quiet "HEAD:$prefix") || [[ $(git cat-file -t "$tree") != tree ]]; then
    echo "$prefix is not a directory at HEAD." >&2
    exit 1
fi

# Prints the subtree split of the prefix after checking that it carries exactly HEAD:DIR.
subtree_split() {
    local split
    split=$(git subtree split --quiet --prefix="$prefix" HEAD)
    if [[ $(git rev-parse "$split^{tree}") != "$tree" ]]; then
        echo "The split of $prefix does not match HEAD:$prefix." >&2
        exit 1
    fi
    printf '%s\n' "$split"
}

if ! $push; then
    if [[ $history == preserve ]]; then
        subtree_split
    else
        printf '%s\n' "$tree"
    fi
    exit 0
fi

if [[ $history == flatten ]]; then
    : "${MIRROR_BOT_NAME:?Set MIRROR_BOT_NAME to the bot author of flattened commits.}"
    : "${MIRROR_BOT_EMAIL:?Set MIRROR_BOT_EMAIL to the bot email of flattened commits.}"
fi
if [[ -z $remote ]]; then
    : "${MIRROR_TOKEN:?Set MIRROR_TOKEN to a token allowed to write $repository.}"
    remote="https://github.com/$repository.git"
    authorization=$(printf 'x-access-token:%s' "$MIRROR_TOKEN" | base64 | tr -d '\n')
    # Environment-only Git configuration keeps the token out of URLs and files.
    export GIT_CONFIG_COUNT=2
    export GIT_CONFIG_KEY_0=credential.helper GIT_CONFIG_VALUE_0=''
    export GIT_CONFIG_KEY_1=http.https://github.com/.extraheader
    export GIT_CONFIG_VALUE_1="AUTHORIZATION: basic $authorization"
    export GIT_TERMINAL_PROMPT=0
fi

mirror_head=""
if [[ -n $(git ls-remote --heads "$remote" refs/heads/main) ]]; then
    git fetch --quiet --no-tags "$remote" refs/heads/main
    mirror_head=$(git rev-parse FETCH_HEAD)
fi

if [[ $history == preserve ]]; then
    publish=$(subtree_split)
    if [[ $mirror_head == "$publish" ]]; then
        echo "$repository is up to date at $publish."
        exit 0
    fi
    if [[ -n $mirror_head ]] && ! git merge-base --is-ancestor "$mirror_head" "$publish"; then
        echo "The main branch of $repository has commits that are not in the history of" \
            "$prefix. Bring them into $prefix here instead of publishing over them." >&2
        exit 1
    fi
else
    parent=()
    if [[ -n $mirror_head ]]; then
        if [[ $(git rev-parse "$mirror_head^{tree}") == "$tree" ]]; then
            echo "$repository is up to date at $mirror_head."
            exit 0
        fi
        # The committer check catches a manual commit, not a forged identity: anyone who can
        # push to the mirror can already rewrite it.
        if [[ $(git log -1 --format=%ce "$mirror_head") != "$MIRROR_BOT_EMAIL" ]]; then
            split=$(subtree_split)
            if ! git merge-base --is-ancestor "$mirror_head" "$split"; then
                echo "The main branch of $repository has commits this script did not publish." \
                    "Bring them into $prefix here instead of publishing over them." >&2
                exit 1
            fi
        fi
        parent=(-p "$mirror_head")
    fi
    [[ -n $message ]] || message="Sync $prefix from ${GITHUB_REPOSITORY:-the source repository} at $source_commit"
    publish=$(GIT_AUTHOR_NAME=$MIRROR_BOT_NAME GIT_AUTHOR_EMAIL=$MIRROR_BOT_EMAIL \
        GIT_COMMITTER_NAME=$MIRROR_BOT_NAME GIT_COMMITTER_EMAIL=$MIRROR_BOT_EMAIL \
        git commit-tree "$tree" ${parent[@]+"${parent[@]}"} -m "$message")
fi

git push --quiet "$remote" "$publish:refs/heads/main"
echo "Published $publish to $repository."
