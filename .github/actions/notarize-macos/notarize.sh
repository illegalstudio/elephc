#!/usr/bin/env bash
# Sign the final executable and require notarization before archive creation.
# Invoked by the sibling composite action on release and nightly macOS jobs.
set -euo pipefail

for name in RUNNER_TEMP NOTARIZE_BINARY APPLE_CERTIFICATE_P12 APPLE_CERTIFICATE_PASSWORD \
    APPLE_SIGNING_IDENTITY APPLE_ID APPLE_TEAM_ID APPLE_APP_PASSWORD; do
    if [[ -z "${!name:-}" ]]; then
        echo "::error::Missing required notarization setting: $name" >&2
        exit 1
    fi
done
if [[ ! -f "$NOTARIZE_BINARY" || ! -x "$NOTARIZE_BINARY" ]]; then
    echo "::error::Notarization requires an executable: $NOTARIZE_BINARY" >&2
    exit 1
fi

umask 077
SIGNING_WORKDIR="$(mktemp -d "$RUNNER_TEMP/elephc-notarization.XXXXXX")"
KEYCHAIN_PATH="$SIGNING_WORKDIR/signing.keychain-db"
ACTIVE_NOTARY_PID=""
ORIGINAL_KEYCHAINS=()
RESTORE_SEARCH_LIST=""

cleanup() {
    local result=$?
    trap - EXIT
    # Stop our worker before deleting the inputs it may still be using. A
    # cancelled submission must not defer cleanup until Apple's wait finishes.
    if [[ -n "$ACTIVE_NOTARY_PID" ]]; then
        kill -KILL "$ACTIVE_NOTARY_PID" 2>/dev/null || true
        wait "$ACTIVE_NOTARY_PID" 2>/dev/null || true
    fi
    if [[ -n "$RESTORE_SEARCH_LIST" ]]; then
        security list-keychains -d user -s ${ORIGINAL_KEYCHAINS[@]+"${ORIGINAL_KEYCHAINS[@]}"} >/dev/null 2>&1 || true
    fi
    security delete-keychain "$KEYCHAIN_PATH" >/dev/null 2>&1 || true
    rm -rf "$SIGNING_WORKDIR"
    exit "$result"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

run_notary() {
    local result=0
    xcrun notarytool "$@" &
    ACTIVE_NOTARY_PID=$!
    # Unlike a foreground external command, the wait builtin lets Bash run its
    # signal trap immediately while the worker is still alive.
    wait "$ACTIVE_NOTARY_PID" || result=$?
    ACTIVE_NOTARY_PID=""
    return "$result"
}

KEYCHAIN_PASSWORD="$(openssl rand -hex 32)"
echo "::add-mask::$KEYCHAIN_PASSWORD"
printf '%s' "$APPLE_CERTIFICATE_P12" | base64 --decode > "$SIGNING_WORKDIR/certificate.p12"
security create-keychain -p "$KEYCHAIN_PASSWORD" "$KEYCHAIN_PATH"
# codesign --keychain only narrows the identity search: the keychain must also
# be on the user search list, or signing fails with errSecItemNotFound.
SEARCH_LIST_OUTPUT="$(security list-keychains -d user)"
while IFS= read -r line; do
    keychain="${line#*\"}"
    keychain="${keychain%\"*}"
    if [[ -n "$keychain" ]]; then
        ORIGINAL_KEYCHAINS+=("$keychain")
    fi
done <<< "$SEARCH_LIST_OUTPUT"
RESTORE_SEARCH_LIST=1
security list-keychains -d user -s "$KEYCHAIN_PATH" ${ORIGINAL_KEYCHAINS[@]+"${ORIGINAL_KEYCHAINS[@]}"}
security set-keychain-settings -lut 3600 "$KEYCHAIN_PATH"
security unlock-keychain -p "$KEYCHAIN_PASSWORD" "$KEYCHAIN_PATH"
security import "$SIGNING_WORKDIR/certificate.p12" \
    -P "$APPLE_CERTIFICATE_PASSWORD" -k "$KEYCHAIN_PATH" -T /usr/bin/codesign
security set-key-partition-list -S apple-tool:,apple:,codesign: -s \
    -k "$KEYCHAIN_PASSWORD" "$KEYCHAIN_PATH"
security find-identity -v -p codesigning "$KEYCHAIN_PATH"

codesign --force --options runtime --timestamp \
    --keychain "$KEYCHAIN_PATH" --sign "$APPLE_SIGNING_IDENTITY" "$NOTARIZE_BINARY"
codesign --verify --strict --verbose=2 "$NOTARIZE_BINARY"

# Apple accepts ZIP submissions. The identical signed bytes go into the tarball
# later. Standalone CLI binaries cannot carry stapled tickets; Gatekeeper looks
# up their notarization tickets online.
mkdir "$SIGNING_WORKDIR/payload"
cp "$NOTARIZE_BINARY" "$SIGNING_WORKDIR/payload/elephc"
ditto -c -k --keepParent "$SIGNING_WORKDIR/payload" "$SIGNING_WORKDIR/elephc.zip"
NOTARY_CREDENTIALS=(--apple-id "$APPLE_ID" --team-id "$APPLE_TEAM_ID" --password "$APPLE_APP_PASSWORD")
SUBMIT_RESULT=0
run_notary submit "$SIGNING_WORKDIR/elephc.zip" "${NOTARY_CREDENTIALS[@]}" \
    --wait --timeout 30m --output-format json > "$SIGNING_WORKDIR/submission.json" || SUBMIT_RESULT=$?
cat "$SIGNING_WORKDIR/submission.json"
SUBMISSION_ID="$(jq -r '.id // empty' "$SIGNING_WORKDIR/submission.json" 2>/dev/null || true)"
STATUS="$(jq -r '.status // empty' "$SIGNING_WORKDIR/submission.json" 2>/dev/null || true)"

if [[ "$SUBMIT_RESULT" -ne 0 || "$STATUS" != Accepted ]]; then
    # Print Apple's diagnostic log before cleanup so it survives in the job log.
    if [[ -n "$SUBMISSION_ID" ]]; then
        if run_notary log "$SUBMISSION_ID" "${NOTARY_CREDENTIALS[@]}" "$SIGNING_WORKDIR/notary-log.json"; then
            cat "$SIGNING_WORKDIR/notary-log.json"
        fi
    fi
    echo "::error::Notarization requires Accepted status; got '${STATUS:-unknown}' (exit $SUBMIT_RESULT)." >&2
    exit 1
fi
echo "Apple accepted notarization submission $SUBMISSION_ID."
