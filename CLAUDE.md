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
cargo test                          # run all tests (slow — ~65s due to as+ld per test)
cargo test -- --include-ignored     # run ALL tests including those requiring external libs
cargo test --test codegen_tests     # run only end-to-end tests
cargo test test_fizzbuzz            # run a specific test
```

Some tests are marked `#[ignore]` because they require external libraries (e.g., SDL2) not available in CI. **Before committing, always run `cargo test -- --include-ignored` locally** to verify nothing is broken — including ignored tests.

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

### Adding a new built-in function

1. Add type signature in `src/types/checker/builtins.rs` (argument count, types, return type)
2. Create a new file in `src/codegen/builtins/<category>/` (e.g., `strings/my_func.rs`)
3. Add `pub mod my_func;` and a match arm in the category's `mod.rs`
4. If the function needs an ARM64 runtime routine, create `src/codegen/runtime/strings/my_func.rs`
5. Add `pub mod` and re-export in `runtime/strings/mod.rs`, call it from `runtime/mod.rs`
6. Add codegen and error tests

Each builtin and runtime file contains exactly **one function**. Never add to an existing file — always create a new one.

### Codegen conventions (ARM64)

- **Integers**: result in `x0`
- **Floats**: result in `d0`
- **Strings**: pointer in `x1`, length in `x2`
- **Function args**: `x0`-`x7` (int = 1 reg, string = 2 regs), `d0`-`d7` (floats)
- **Return value**: same as expression result (`x0`, `d0`, or `x1`/`x2`)
- **Stack frame**: `x29` = frame pointer, `x30` = link register, locals at negative offsets from `x29`
- **ABI helpers**: `src/codegen/abi.rs` centralizes load/store/write per type
- **Labels**: use `ctx.next_label("prefix")` — global counter prevents collisions across functions

### Assembly comment policy

**Every `emitter.instruction(...)` call MUST have an inline `//` comment** explaining what the ARM64 instruction does. This is mandatory — the codebase is educational and every assembly line must be understandable by someone learning how compilers work.

Rules:

1. **Every instruction line gets a comment.** No exceptions. If you add a new `emitter.instruction(...)`, it must have a `// comment`.
2. **Alignment: `//` starts at column 81.** Pad with spaces so the `//` is at the 81st character position (1-indexed). If the code itself is >= 80 characters, add exactly one space before `//`.
3. **Block comments before related groups.** Use `// -- description --` on a standalone line before a block of related instructions (e.g., `// -- set up stack frame --`, `// -- copy bytes from source --`).
4. **Comments explain intent, not mnemonics.** Write "store argc from OS" not "store x0 to memory". The reader can see the instruction — explain *why* it's there.

Example of correct formatting:

```rust
    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #32");                                 // allocate 32 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #16]");                        // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                // set new frame pointer

    // -- convert integer to string and write to stdout --
    emitter.instruction("bl __rt_itoa");                                    // convert x0 to decimal string → x1=ptr, x2=len
    emitter.instruction("mov x0, #1");                                     // fd = stdout
    emitter.instruction("mov x16, #4");                                    // syscall 4 = sys_write
    emitter.instruction("svc #0x80");                                      // invoke macOS kernel
```

To verify alignment, run:
```bash
python3 -c "
with open('path/to/file.rs') as f:
    for i, line in enumerate(f, 1):
        if 'emitter.instruction' in line and '//' in line:
            pos = line.rstrip().index('//')
            if pos != 80 and len(line[:pos].rstrip()) < 80:
                print(f'Line {i}: // at col {pos+1}')
"
```

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
