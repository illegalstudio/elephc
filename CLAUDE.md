# elephc — Developer Guide

## What is this

A PHP-to-native compiler written in Rust. Compiles a static subset of PHP to ARM64 assembly, producing standalone macOS Mach-O binaries. No interpreter, no VM, no runtime dependencies.

## Build & run

```bash
cargo build              # dev build
cargo build --release    # optimized build
cargo run -- file.php    # compile a PHP file
```

The compiler outputs a native binary next to the source file (e.g., `file.php` → `file`).

## Test policy

**Every feature must have tests before it's considered done.** The test suite is the primary quality gate.

### Running tests

```bash
cargo test               # run all tests (slow — ~65s due to as+ld per test)
cargo test --test codegen_tests  # run only end-to-end tests
cargo test test_fizzbuzz  # run a specific test
```

### Test strategy during development

The full test suite is slow because each codegen test spawns `as` + `ld` + runs the binary. To avoid waiting 60+ seconds on every change:

1. **While developing a feature**: run only the tests for that feature (`cargo test test_my_feature`)
2. **When the feature is complete**: run the full suite once (`cargo test`) to check for regressions
3. **PHP cross-check**: opt-in via `ELEPHC_PHP_CHECK=1 cargo test` — verifies output matches PHP interpreter

### Test structure

| File | What it tests | How |
|---|---|---|
| `tests/lexer_tests.rs` | Tokenization | Asserts token sequences from source strings |
| `tests/parser_tests.rs` | AST construction | Asserts AST node structure and operator precedence |
| `tests/codegen_tests.rs` | Full pipeline (end-to-end) | Compiles PHP → binary, runs it, asserts stdout |
| `tests/error_tests.rs` | Error reporting | Asserts that invalid programs produce the right error messages |

### Test coverage requirements

- **New language construct** (keyword, operator, statement): needs lexer, parser, codegen, AND error tests
- **New operator**: needs a Pratt parser binding power test verifying precedence relative to adjacent operators
- **New statement type**: needs at least one codegen test showing correct output, one test for edge cases (empty body, nested), and one error test for malformed syntax
- **New built-in function**: needs codegen tests for normal use and error test for wrong argument count/types
- **Bug fix**: must include a regression test that would have caught the bug
- **Every feature also needs an example** in `examples/`. If an existing example can showcase the new feature naturally, update it. Otherwise, create a new `examples/<name>/main.php` with its own `.gitignore` (containing `*.s`, `*.o`, `main`). Examples should be small, readable programs that demonstrate real use cases — not just test cases.

### Writing codegen tests

Codegen tests compile inline PHP source and assert stdout:

```rust
#[test]
fn test_my_feature() {
    let out = compile_and_run("<?php echo 1 + 2;");
    assert_eq!(out, "3");
}
```

Each test runs in an isolated temp directory. Tests run in parallel — the `compile_and_run` helper handles isolation automatically.

## Architecture

```
PHP source → Lexer (tokens) → Parser (AST) → Resolver (include/require) → Type Checker → Codegen (ARM64 asm) → as + ld → binary
```

### Key modules

| Module | Entry point | Responsibility |
|---|---|---|
| `src/lexer/` | `tokenize()` | Source → `Vec<(Token, Span)>` |
| `src/parser/` | `parse()` | Tokens → `Program` (Vec of Stmt). Pratt parser for expressions |
| `src/resolver.rs` | `resolve()` | Resolves `include`/`require` by inlining referenced files. Runs between parse and type check |
| `src/types/` | `check()` | Type checking, returns `CheckResult` with `TypeEnv` + `FunctionSig` map |
| `src/codegen/` | `generate()` | AST → ARM64 assembly string. Emits function bodies + `_main` |
| `src/errors/` | `report()` | Error formatting with line:col |
| `src/span.rs` | `Span` | Source position (line, col) attached to all AST nodes |

### Adding a new operator

1. Add token to `src/lexer/token.rs`
2. Add scanning logic to `src/lexer/scan.rs`
3. Add `BinOp` variant to `src/parser/ast.rs`
4. Add one line to `infix_bp()` in `src/parser/expr.rs` (the Pratt parser binding power table)
5. Add type checking in `src/types/checker.rs`
6. Add ARM64 codegen in `src/codegen/expr.rs`
7. Add tests in all 4 test files

### Adding a new statement type

1. Add `StmtKind` variant to `src/parser/ast.rs`
2. Add parser logic in `src/parser/stmt.rs`
3. Add type checking in `src/types/checker.rs`
4. Add codegen in `src/codegen/stmt.rs`
5. If it introduces variables, update `collect_local_vars` in `src/codegen/functions.rs`
6. Add tests

### Codegen conventions (ARM64)

- **Integers**: result in `x0`
- **Strings**: pointer in `x1`, length in `x2`
- **Function args**: `x0`-`x7` (int = 1 reg, string = 2 regs)
- **Return value**: same as expression result (`x0` or `x1`/`x2`)
- **Stack frame**: `x29` = frame pointer, `x30` = link register, locals at negative offsets from `x29`
- **ABI helpers**: `src/codegen/abi.rs` centralizes load/store/write per type
- **Labels**: use `ctx.next_label("prefix")` — global counter prevents collisions across functions

## Examples

Each example lives in `examples/<name>/main.php` with its own `.gitignore`. To run:

```bash
cargo run -- examples/fizzbuzz/main.php
./examples/fizzbuzz/main
```

## PHP compatibility

**The syntax must be 100% compatible with PHP.** Any valid elephc program must also be valid PHP and produce the same output when run with `php`. This means:

- Variable names, keywords, operators, and built-in function names must match PHP exactly
- Superglobals (`$argc`, `$argv`) must use PHP's syntax (e.g., `$argv[0]`, not `argv(0)`)
- Operator precedence and associativity must match PHP
- String escape sequences must match PHP behavior
- Built-in function signatures must match PHP (argument count, order, types)

When in doubt, test with `php -r '...'` to verify behavior.

## Documentation

The `docs/` directory contains the project documentation:

- `docs/language-reference.md` — What elephc supports: types, operators, control structures, functions, built-ins, limitations, and known incompatibilities with PHP. Includes examples of what works and what doesn't.
- `docs/architecture.md` — Compiler internals: pipeline, module map, ARM64 conventions, memory layout.

**Documentation must be kept up to date.** When adding a new feature:
1. Add it to `docs/language-reference.md` — in the relevant section (operators, functions, built-ins, etc.)
2. If it was previously listed as "not supported", remove that note
3. If there are known incompatibilities with PHP, document them
4. Update `docs/architecture.md` if the change affects the pipeline or module structure

## Roadmap management

`ROADMAP.md` tracks all planned and completed work, organized by version.

- **Never remove completed items** from a version section. Mark them as `[x]` and leave them under the version they belong to. This preserves the history of what was delivered in each release.
- New work items go under the appropriate future version.
- When all items in a version are completed, the version is considered done — do not move items elsewhere.

## Conventions

- No `Co-Authored-By` lines in commits
- Keep commit messages concise
- Run `cargo test` before committing — all tests must pass
- Zero compiler warnings policy (`cargo build` must be clean)
