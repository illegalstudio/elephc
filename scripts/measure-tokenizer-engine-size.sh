#!/usr/bin/env bash
#
# Measures the engine-only tokenizer binary, including the optional AST parser
# that TOKEN_PARSE uses.
#
# Usage:
#   ./scripts/measure-tokenizer-engine-size.sh size [source.php]
#   ./scripts/measure-tokenizer-engine-size.sh fast [source.php]
#
# `size` uses opt-level z; `fast` keeps the lexer at opt-level 3 and compiles the
# validation-only parser at opt-level z, because TOKEN_PARSE is not the lexer's hot
# path. Both use fat LTO, one codegen unit, symbol stripping and
# `panic = "immediate-abort"`, which removes the std panic/backtrace formatter from
# the executable. The parser is enabled, so this is the full TOKEN_PARSE engine,
# not the lexer-only default build.
#
# The panic strategy is unstable in the pinned toolchain, so the probe is built
# in a throwaway Cargo project with `-Z build-std=std,panic_abort`. This keeps the
# workspace manifest free of the `panic-immediate-abort` cargo feature. The build
# needs the `rust-src` component; the script prints the rustup command when it is
# missing.
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
ENGINE_DIR="$(cd "$PROJECT_DIR/crates/elephc-tokenizer-engine" && pwd -P)"
TOOLCHAIN="${RUST_TOOLCHAIN:-1.98}"

MODE="size"
FILE=""
for arg in "$@"; do
  case "$arg" in
    size|fast)
      MODE="$arg"
      ;;
    -*)
      echo "unknown option: $arg" >&2
      exit 2
      ;;
    *)
      if [ -n "$FILE" ]; then
        echo "only one source file may be measured" >&2
        exit 2
      fi
      FILE="$arg"
      ;;
  esac
done

if ! rustup component list --toolchain "$TOOLCHAIN" 2>/dev/null | grep -q '^rust-src (installed)'; then
  echo "rust-src is not installed for toolchain $TOOLCHAIN." >&2
  echo "Run: rustup component add rust-src --toolchain $TOOLCHAIN" >&2
  exit 1
fi

CARGO=(cargo)
if rustup run "$TOOLCHAIN" cargo --version >/dev/null 2>&1; then
  CARGO=(rustup run "$TOOLCHAIN" cargo)
fi

WORK="$(mktemp -d "${TMPDIR:-/tmp}/elephc-engine-size.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT
mkdir -p "$WORK/src"

cat > "$WORK/Cargo.toml" <<EOF
cargo-features = ["panic-immediate-abort"]

[package]
name = "engine-size"
version = "0.1.0"
edition = "2021"
publish = false

[workspace]

[dependencies]
elephc-tokenizer-engine = { path = "$ENGINE_DIR", features = ["parse"] }

[profile.size]
inherits = "release"
opt-level = "z"
lto = "fat"
codegen-units = 1
panic = "immediate-abort"
strip = "symbols"

[profile.fast]
inherits = "release"
lto = "fat"
codegen-units = 1
panic = "immediate-abort"
strip = "symbols"

[profile.fast.package.elephc-syntax-validate]
opt-level = "z"
EOF

cat > "$WORK/src/main.rs" <<'EOF'
//! Minimal engine-only size probe: tokenizes a file with TOKEN_PARSE enabled.
fn main() {
    let source = std::env::args()
        .nth(1)
        .map(|path| std::fs::read(path).expect("read source"))
        .unwrap_or_else(|| b"<?php echo 1 + 2;".to_vec());
    let output = elephc_tokenizer_engine::tokenize_with_warnings(
        &source,
        elephc_tokenizer_engine::TOKEN_PARSE,
    )
    .expect("tokenize");
    println!("{}", output.tokens.len());
}
EOF

(
  cd "$WORK"
  RUSTC_BOOTSTRAP=1 "${CARGO[@]}" -Z build-std=std,panic_abort build --profile "$MODE"
)

BIN="$WORK/target/$MODE/engine-size"
SIZE="$(wc -c < "$BIN" | tr -d ' ')"
echo "$MODE engine: $SIZE bytes"
if [ -n "$FILE" ]; then
  "$BIN" "$FILE"
fi
