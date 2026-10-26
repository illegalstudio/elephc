#!/usr/bin/env bash
# Focused tests for publish_subtree_mirror.sh. A synthetic source repository publishes its
# `app` directory to local bare mirrors through --remote, so no network or token is needed.
set -euo pipefail

script="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)/publish_subtree_mirror.sh"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT HUP INT TERM

# The user's Git configuration (signing, hooks, default branch) must not leak in.
export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1
export GIT_AUTHOR_NAME=Developer GIT_AUTHOR_EMAIL=developer@example.com
export GIT_COMMITTER_NAME=Developer GIT_COMMITTER_EMAIL=developer@example.com
export MIRROR_BOT_NAME='mirror-bot[bot]' MIRROR_BOT_EMAIL='1+mirror-bot[bot]@users.noreply.github.com'
unset MIRROR_TOKEN GITHUB_REPOSITORY

source="$work/source"
mirror="$work/mirror.git"
passed=0

fail() {
    echo "FAIL: $*" >&2
    exit 1
}

ok() {
    echo "ok - $1"
    passed=$((passed + 1))
}

# Commits CONTENT to PATH in REPO with MESSAGE.
commit_file() {
    mkdir -p "$(dirname "$1/$2")"
    printf '%s\n' "$3" > "$1/$2"
    git -C "$1" add "$2"
    git -C "$1" commit --quiet -m "$4"
}

# Runs the script from the source repository against the local mirror.
publish() {
    (cd "$source" && bash "$script" --prefix app --repository example/mirror --remote "$mirror" "$@")
}

# Runs the script and fails the test unless it stops with a message containing EXPECTED.
# Checking the message matters: a plain `git push` already refuses a non-fast-forward, so
# only the message shows that the script's own check stopped it.
publish_must_fail() {
    local expected=$1
    shift
    if publish "$@" > "$work/output" 2>&1; then
        fail "publish $* succeeded: $(cat "$work/output")"
    fi
    grep -qF "$expected" "$work/output" || fail "publish $* failed without \"$expected\": $(cat "$work/output")"
}

mirror_main() {
    git -C "$mirror" rev-parse --verify --quiet refs/heads/main || true
}

# Commits directly on the mirror, the way a manual push there would.
commit_on_mirror() {
    rm -rf "$work/clone"
    git clone --quiet "$mirror" "$work/clone"
    commit_file "$work/clone" manual.txt manual "manual commit on the mirror"
    git -C "$work/clone" push --quiet origin HEAD:main
}

git init --quiet -b main "$source"
commit_file "$source" README.md readme "add the readme"
commit_file "$source" app/a.txt a "add a"
commit_file "$source" other/x.txt x "add an unrelated file"
commit_file "$source" app/b.txt b "add b"
git init --quiet --bare -b main "$mirror"

dry=$(publish)
[[ $(git -C "$source" rev-parse "$dry^{tree}") == $(git -C "$source" rev-parse HEAD:app) ]] ||
    fail "the dry run did not print the split of app"
[[ -z $(mirror_main) ]] || fail "the dry run wrote to the mirror"
[[ $(publish --history flatten) == $(git -C "$source" rev-parse HEAD:app) ]] ||
    fail "the flatten dry run did not print the tree of app"
ok "a dry run prints what it would publish and leaves the mirror alone"

publish --push > /dev/null
[[ $(mirror_main) == "$dry" ]] || fail "preserve did not publish the split"
[[ $(git -C "$mirror" rev-list --count main) == 2 ]] || fail "the mirror history is not the history of app"
[[ $(git -C "$mirror" log -1 --format='%s %ae' main) == "add b developer@example.com" ]] ||
    fail "preserve did not keep the original message and author"
ok "preserve publishes the history of the directory with its authors"

[[ $(publish --push) == *"up to date"* ]] || fail "an unchanged source was published again"
[[ $(mirror_main) == "$dry" ]] || fail "an unchanged source moved the mirror"
ok "preserve does nothing when the mirror is up to date"

printf 'draft\n' > "$source/app/untracked.txt"
commit_file "$source" app/c.txt c "add c"
publish --push > /dev/null
git -C "$mirror" merge-base --is-ancestor "$dry" main || fail "preserve did not fast-forward the mirror"
[[ $(git -C "$mirror" rev-list --count main) == 3 ]] || fail "preserve did not add the new commit"
git -C "$mirror" cat-file -e main:untracked.txt 2> /dev/null && fail "an uncommitted file was published"
rm "$source/app/untracked.txt"
ok "preserve fast-forwards and never publishes uncommitted files"

published=$(mirror_main)
commit_on_mirror
diverged=$(mirror_main)
commit_file "$source" app/d.txt d "add d"
publish_must_fail "commits that are not in the history of app" --push
[[ $(mirror_main) == "$diverged" ]] || fail "preserve overwrote a commit made on the mirror"
git -C "$mirror" update-ref refs/heads/main "$published"
ok "preserve refuses to publish over a commit made on the mirror"

publish --history flatten --push > /dev/null
flattened=$(mirror_main)
[[ $(git -C "$mirror" rev-parse "$flattened^") == "$published" ]] ||
    fail "the first flatten commit is not on top of the preserved history"
[[ $(git -C "$mirror" log -1 --format='%an <%ae> %cn <%ce>' main) == \
    "mirror-bot[bot] <1+mirror-bot[bot]@users.noreply.github.com> mirror-bot[bot] <1+mirror-bot[bot]@users.noreply.github.com>" ]] ||
    fail "the flatten commit is not authored and committed by the bot"
[[ $(git -C "$mirror" rev-parse "main^{tree}") == $(git -C "$source" rev-parse HEAD:app) ]] ||
    fail "the flatten commit does not carry the tree of app"
[[ $(git -C "$mirror" log -1 --format=%s main) == \
    "Sync app from the source repository at $(git -C "$source" rev-parse HEAD)" ]] ||
    fail "the flatten commit does not use the default message"
ok "a mirror switches from preserve to flatten on top of its history"

[[ $(publish --history flatten --push) == *"up to date"* ]] || fail "an unchanged source was flattened again"
[[ $(mirror_main) == "$flattened" ]] || fail "an unchanged source moved the flattened mirror"
commit_file "$source" app/e.txt e "add e"
commit_file "$source" app/f.txt f "add f"
publish --history flatten --message "Update" --push > /dev/null
[[ $(git -C "$mirror" rev-parse main^) == "$flattened" ]] || fail "flatten did not add one commit on top"
[[ $(git -C "$mirror" log -1 --format=%s main) == "Update" ]] || fail "flatten ignored --message"
ok "flatten adds one commit per publish and does nothing when up to date"

flattened=$(mirror_main)
commit_on_mirror
diverged=$(mirror_main)
commit_file "$source" app/g.txt g "add g"
publish_must_fail "commits this script did not publish" --history flatten --push
[[ $(mirror_main) == "$diverged" ]] || fail "flatten overwrote a commit made on the mirror"
git -C "$mirror" update-ref refs/heads/main "$flattened"
ok "flatten refuses to publish over a commit made on the mirror"

publish_must_fail "commits that are not in the history of app" --push
[[ $(mirror_main) == "$flattened" ]] || fail "preserve rewrote a flattened mirror"
ok "a flattened mirror cannot switch back to preserve"

mirror="$work/empty.git"
git init --quiet --bare -b main "$mirror"
publish --history flatten --push > /dev/null
[[ $(git -C "$mirror" rev-list --count main) == 1 ]] || fail "flatten on an empty mirror made more than one commit"
[[ $(git -C "$mirror" rev-parse "main^{tree}") == $(git -C "$source" rev-parse HEAD:app) ]] ||
    fail "flatten on an empty mirror does not carry the tree of app"
ok "flatten starts an empty mirror with one commit"

if (cd "$source" && bash "$script" --prefix missing --repository example/mirror) > /dev/null 2>&1; then
    fail "a missing directory was accepted"
fi
if (cd "$source" && bash "$script" --prefix app --repository example/mirror --push) > "$work/output" 2>&1; then
    fail "a GitHub publish without MIRROR_TOKEN was attempted"
fi
grep -q MIRROR_TOKEN "$work/output" || fail "the missing token was not reported"
if (cd "$source" && bash "$script" --prefix app --repository example/mirror --history squash) > /dev/null 2>&1; then
    fail "an unknown history mode was accepted"
fi
ok "a missing directory, token or history mode stops the script"

echo "$passed tests passed"
