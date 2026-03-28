#!/bin/bash
# Build all examples
# Usage: ./examples/build-all.sh

set -e

if [ ! -f "Cargo.toml" ]; then
    echo "Error: run this script from the project root (where Cargo.toml is)."
    exit 1
fi

ELEPHC="cargo run --release --"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PASS=0
FAIL=0
ERRORS=""

sdl_link_args=()
for candidate in "/opt/homebrew/lib" "/usr/local/lib"; do
    if [ -f "$candidate/libSDL2.dylib" ] || [ -f "$candidate/libSDL2.a" ]; then
        sdl_link_args=(-l SDL2 -L "$candidate")
        break
    fi
done

for dir in "$SCRIPT_DIR"/*/; do
    name=$(basename "$dir")
    if [ -f "$dir/main.php" ]; then
        printf "  %-25s" "$name"
        example_args=()
        if [[ "$name" == sdl_* ]]; then
            if [ ${#sdl_link_args[@]} -eq 0 ]; then
                echo "FAIL"
                FAIL=$((FAIL + 1))
                ERRORS="$ERRORS  $name (SDL2 library not found in /opt/homebrew/lib or /usr/local/lib)\n"
                continue
            fi
            example_args=("${sdl_link_args[@]}")
        fi

        if $ELEPHC "${example_args[@]}" "$dir/main.php" >/dev/null 2>&1; then
            echo "ok"
            PASS=$((PASS + 1))
        else
            echo "FAIL"
            FAIL=$((FAIL + 1))
            ERRORS="$ERRORS  $name\n"
        fi
    fi
done

echo ""
echo "Results: $PASS passed, $FAIL failed"
if [ $FAIL -gt 0 ]; then
    echo "Failed:"
    printf "$ERRORS"
    exit 1
fi
