#!/bin/sh
# Focused portability and corruption tests for the CI nextest artifact gate.
set -eu

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
fixture_dir=$(mktemp -d "$PWD/.nextest-artifact-test.XXXXXX")
trap 'rm -rf "$fixture_dir"' EXIT HUP INT TERM

printf '#!/bin/sh\n[ "$*" = "nextest --version" ]\n' > "$fixture_dir/cargo-nextest"
(cd "$fixture_dir" && shasum -a 256 cargo-nextest > cargo-nextest.sha256)
sh "$script_dir/validate_archived_nextest.sh" "$fixture_dir"
test -x "$fixture_dir/cargo-nextest"

# A checksum mismatch must reject the binary before chmod or execution.
chmod 0644 "$fixture_dir/cargo-nextest"
printf 'corrupt\n' >> "$fixture_dir/cargo-nextest"
if sh "$script_dir/validate_archived_nextest.sh" "$fixture_dir"; then
    echo 'corrupt binary unexpectedly accepted' >&2
    exit 1
fi
test ! -x "$fixture_dir/cargo-nextest"

# A missing executable is a recoverable first-attempt failure, not a prepare-step failure.
rm "$fixture_dir/cargo-nextest"
if sh "$script_dir/validate_archived_nextest.sh" "$fixture_dir"; then
    echo 'missing binary unexpectedly accepted' >&2
    exit 1
fi

# Simulate a successful second artifact extraction.
printf '#!/bin/sh\n[ "$*" = "nextest --version" ]\n' > "$fixture_dir/cargo-nextest"
sh "$script_dir/validate_archived_nextest.sh" "$fixture_dir"

rm "$fixture_dir/cargo-nextest.sha256"
if sh "$script_dir/validate_archived_nextest.sh" "$fixture_dir"; then
    echo 'missing checksum unexpectedly accepted' >&2
    exit 1
fi

# Matching bytes still need to pass the executable/version smoke check.
printf '#!/bin/sh\nexit 1\n' > "$fixture_dir/cargo-nextest"
(cd "$fixture_dir" && shasum -a 256 cargo-nextest > cargo-nextest.sha256)
if sh "$script_dir/validate_archived_nextest.sh" "$fixture_dir"; then
    echo 'invalid executable unexpectedly accepted' >&2
    exit 1
fi

echo 'nextest artifact validation tests passed'
