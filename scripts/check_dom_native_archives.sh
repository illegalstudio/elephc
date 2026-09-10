#!/usr/bin/env bash
set -euo pipefail

# Validate the production/test archive boundary without rebuilding the DOM bridge.
if [[ $# -ne 2 ]]; then
    echo "usage: $0 <production-archive> <test-archive>" >&2
    exit 2
fi

production_archive=$1
test_archive=$2
script_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
build_script="$script_root/crates/elephc-dom/build.rs"

production_test_source_count=$(awk '
    /^fn build_lexbor\(/ { in_production = 1 }
    /^fn build_test_instrumentation\(/ { in_production = 0 }
    in_production && index($0, ".file(manifest.join(\"native/test_instrumentation.c\"))") { count++ }
    END { print count + 0 }
' "$build_script")
test_source_count=$(awk '
    /^fn build_test_instrumentation\(/ { in_test = 1 }
    /^fn emit_platform_links\(/ { in_test = 0 }
    in_test && index($0, ".file(manifest.join(\"native/test_instrumentation.c\"))") { count++ }
    END { print count + 0 }
' "$build_script")
if [[ "$production_test_source_count" != 0 ]]; then
    echo "error: production build still compiles native/test_instrumentation.c" >&2
    exit 1
fi
if [[ "$test_source_count" != 1 ]]; then
    echo "error: test archive source must appear exactly once in its dedicated builder" >&2
    exit 1
fi
rg -n '\.cargo_metadata\(false\)' "$build_script" >/dev/null
rg -n 'compile\("elephc_dom_native_test"\)' "$build_script" >/dev/null

production_symbols=$(nm -g "$production_archive")
if grep -Eq 'elephc_dom_native_test_|elephc_dom_test_' <<<"$production_symbols"; then
    echo "error: production DOM archive contains test instrumentation symbols" >&2
    exit 1
fi

test_symbols=$(nm -g "$test_archive")
grep -Eq 'elephc_dom_native_test_install_resource_loader_allocator' <<<"$test_symbols"
grep -Eq 'elephc_dom_native_test_resource_loader_input_from_io_failure' <<<"$test_symbols"
