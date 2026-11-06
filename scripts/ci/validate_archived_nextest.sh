#!/bin/sh
# Validate extracted nextest content before executing it. The first CI attempt
# may fail recoverably so missing/corrupt binaries trigger another download;
# the retry runs the same checks without continue-on-error.
set -eu

if [ "$#" -ne 1 ]; then
    echo 'usage: validate_archived_nextest.sh <artifact-directory>' >&2
    exit 2
fi

cd "$1"
shasum -a 256 -c cargo-nextest.sha256
chmod 0755 cargo-nextest
./cargo-nextest nextest --version
