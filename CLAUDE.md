# elephc — Developer Guide

## What is this

A PHP-to-native compiler written in Rust. Compiles a static subset of PHP to native assembly for the supported target matrix, producing standalone binaries. No interpreter, no VM, no runtime dependencies.

## Supported target policy

All supported targets are first-class targets. The supported target matrix is currently `macos-aarch64`, `linux-aarch64`, and `linux-x86_64`.

Do not design or land codegen/runtime features as ARM64-first with x86_64 treated as a later port. New features, builtins, runtime helpers, optimizer assumptions that affect emitted code, ABI behavior, and ownership/GC paths must either support every supported target in the same change or clearly isolate an intentionally unsupported path with diagnostics, tests, and documentation. A feature is not considered done while any supported target has a missing runtime symbol, reduced semantics, stale documentation, or an untested target-specific lowering path.

When examples or internals docs use ARM64 snippets for readability, treat them as examples only. Implementation work must keep the target-aware ABI/runtime boundaries authoritative.

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
cargo test                          # run all tests (slow — ~9 min due to as+ld per codegen test)
cargo test -- --include-ignored     # run ALL tests including those requiring external libs
cargo test --test codegen_tests     # run only end-to-end tests
cargo test test_fizzbuzz            # run a specific test
```

Linux target-specific regressions can be checked through the Docker scripts in
`scripts/`. They support both full suites and normal `cargo test` filters:

```bash
./scripts/test-linux-x86_64.sh                     # run all tests on Linux x86_64
./scripts/test-linux-arm64.sh                      # run all tests on Linux ARM64
./scripts/test-linux-x86_64.sh iterable            # run tests matching a filter
./scripts/test-linux-arm64.sh test_my_feature      # run tests matching a filter
./scripts/test-linux-x86_64.sh --rebuild           # rebuild the Docker image first
./scripts/test-linux-arm64.sh --rebuild            # rebuild the Docker image first
```

Some tests are marked `#[ignore]` because they require external libraries (e.g., SDL2) not available in CI. **Before committing, always run `cargo test -- --include-ignored` locally** to verify nothing is broken — including ignored tests.

### Test strategy during development

The full test suite is slow because each codegen test spawns `as` + `ld` + runs the binary. To avoid waiting several minutes on every change:

1. **While developing a feature**: run only the tests for that feature (`cargo test test_my_feature`)
2. **When the feature is complete**: run the full suite once (`cargo test`) to check for regressions
3. **PHP cross-check**: opt-in via `ELEPHC_PHP_CHECK=1 cargo test` — verifies output matches PHP interpreter

### Pre-commit verification

Before committing code changes, run the smallest useful focused tests first, then the full gates:

```bash
cargo build
cargo test <feature_or_regression_filter>
cargo test
cargo test -- --include-ignored
git diff --check
```

For codegen changes, also verify assembly-comment coverage/alignment for any files you touched. If the change can affect generated assembly, runtime helpers, ABI behavior, linking, ownership/GC, or target-specific libraries, run focused tests for each affected supported target. Use the Docker Linux scripts for Linux x86_64 and Linux ARM64 coverage; keep macOS ARM64 covered with local `cargo test` or focused local codegen tests as appropriate.

### Test structure

| File | What it tests | How |
|---|---|---|
| `tests/lexer_tests.rs` | Tokenization | Asserts token sequences from source strings |
| `tests/parser_tests.rs` | AST construction | Asserts AST node structure and operator precedence |
| `tests/codegen_tests.rs`, `tests/codegen/` | Full pipeline (end-to-end) | Compiles PHP → binary, runs it, asserts stdout |
| `tests/error_tests.rs`, `tests/error_tests/` | Error reporting | Asserts that invalid programs produce the right error messages |

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
PHP source → Lexer → Parser → Magic constants → Conditional compilation → Resolver
→ NameResolver → Constant folding → Type Checker / warnings → Optimizer passes
→ Codegen → runtime cache → assembler/linker → binary
```

### Key modules

| Module | Entry point | Responsibility |
|---|---|---|
| `src/pipeline.rs` | `compile()` | Orchestrates frontend passes, type checking, optimization, runtime cache, codegen, and linking |
| `src/lexer/` | `tokenize()` | Source → `Vec<(Token, Span)>` |
| `src/parser/` | `parse()` | Tokens → `Program` (Vec of Stmt). Pratt parser for expressions |
| `src/magic_constants/`, `src/magic_constants.rs` | `substitute_file_and_scope_constants()` | Lowers PHP magic constants before resolver/name-resolver/optimizer passes |
| `src/conditional.rs` | `apply()` | Applies compiler `ifdef` conditional branches |
| `src/resolver/` | `resolve()` | Resolves `include`/`require`, discovers declarations, and tracks include-loaded function variants. Runs before namespace/name canonicalization |
| `src/name_resolver/` | `resolve()` | Applies namespace/use rules, rewrites references to canonical fully-qualified names, handles PHP-style builtin fallback, and flattens namespace-only AST nodes before type checking |
| `src/types/` | `check()` | Type checking, returns `CheckResult` with `TypeEnv`, function/class/interface/enum/FFI metadata, warnings, required libraries, and the internal `Mixed` type for heterogeneous assoc-array values |
| `src/optimize/`, `src/optimize.rs` | `fold_constants()`, `propagate_constants()`, `eliminate_dead_code()` | Constant folding/propagation, control-flow pruning/normalization, DCE, and effect modeling |
| `src/codegen/` | `generate()` / `generate_user_asm()` | AST → target assembly string. Top-level orchestration lives in `mod.rs`, while most lowering lives under `expr/`, `stmt/`, and `runtime/` |
| `src/codegen/abi/` | ABI helpers | Target-specific argument materialization, frame layout, registers, stack slots, symbols, and call helpers |
| `src/codegen/program_usage/` | Program scans | Collects codegen metadata such as required classes and variables before emission |
| `src/runtime_cache.rs` | `prepare_runtime_object()` | Builds/reuses the target runtime object before final linking |
| `src/errors/` | `report()` | Error formatting with line:col |
| `src/span.rs` | `Span` | Source position (line, col) attached to all AST nodes |

### Codegen layout

- `src/codegen/expr.rs` is mainly a dispatcher; most expression lowering now lives in focused helpers under `src/codegen/expr/`
- `src/codegen/stmt.rs` is mainly a dispatcher; most statement lowering now lives in focused helpers under `src/codegen/stmt/`
- `src/codegen/runtime/mod.rs` emits runtime code (`__rt_*` routines)
- `src/codegen/runtime/data.rs` emits runtime `.data` / `.bss` symbols and metadata tables
- `src/codegen/context.rs` carries variable layout, ownership state, class metadata, and FFI metadata through codegen
- `src/codegen/abi/` centralizes target-specific register, stack, frame, symbol, and call mechanics. Prefer these helpers over hardcoding ARM64 or x86_64 details in feature emitters.

### Adding a new operator

1. Add token to `src/lexer/token.rs`
2. Add scanning logic to `src/lexer/scan.rs`
3. Add `BinOp` variant to `src/parser/ast.rs`
4. Add one line to `infix_bp()` in `src/parser/expr/pratt.rs` (the Pratt parser binding power table)
5. Add type checking/inference in the relevant `src/types/checker/` file, usually under `inference/ops.rs` or expression inference
6. Add optimizer/effect handling when the operator can be folded, propagated, pruned, or has side effects
7. Add target-aware codegen in the relevant file under `src/codegen/expr/` (and only touch `src/codegen/expr.rs` if the dispatcher must learn about a new helper path)
8. Add tests in all 4 test files

### Adding a new statement type

1. Add `StmtKind` variant to `src/parser/ast.rs`
2. Add parser logic in `src/parser/stmt.rs`
3. Add resolver/name-resolver handling if the statement can contain names, declarations, includes, function variants, or expressions
4. Add type checking in the relevant `src/types/checker/` module
5. Add optimizer/effects/warnings handling if the statement can be folded, pruned, read variables, write variables, or alter control flow
6. Add codegen in the relevant file under `src/codegen/stmt/` (and only touch `src/codegen/stmt.rs` if the dispatcher must learn about a new helper path)
7. If it introduces variables or hidden temporaries, update local collection in `src/codegen/functions/locals.rs` and any main-emission allocation needed before frame sizing
8. Add tests

### Adding or changing an AST node

When adding a new `ExprKind` or `StmtKind`, check every AST-walking pass. The compiler has many passes that deliberately recurse by variant, and missing one usually creates silent miscompilation rather than a compile error.

Common places to audit:

- Parser construction and lowering in `src/parser/`
- Resolver/include discovery and function-variant handling in `src/resolver/`
- Namespace/use/FQN rewriting in `src/name_resolver/`
- Type checking, inference, return analysis, warnings, and type compatibility in `src/types/`
- Optimizer folding, propagation, DCE, control-flow normalization, and effect modeling in `src/optimize/`
- Program usage scans in `src/codegen/program_usage/`
- Local/hidden-slot collection in `src/codegen/functions/locals.rs`
- Ownership cleanup helpers in `src/codegen/expr/ownership.rs` and related runtime/GC paths
- Expression/statement codegen dispatchers plus the focused lowering module
- Lexer/parser/codegen/error/regression tests, depending on the surface area

### Adding a new built-in function

1. Add the function to `src/types/checker/builtins/catalog.rs`. This is mandatory: it drives PHP-style case-insensitive builtin lookup, namespace fallback, redeclaration checks, and `name_resolver` behavior.
2. Confirm `function_exists("...")` recognizes the function. The implementation delegates to the canonical catalog; do not add a second builtin-name table without keeping it in lockstep.
3. Add or update the call signature in `src/types/signatures.rs`. This is the contract for named arguments: parameter names, default values, variadic name, by-ref/ref-like params, and arity must match PHP. Mark mutating parameters in `ref_params`; named-argument lowering and hidden-temp allocation depend on it.
4. Add type signature handling in the appropriate `src/types/checker/builtins/<category>.rs` file (argument count, value types, return type, warnings, required Linux libraries).
5. Add first-class callable support in `first_class_callable_builtin_sig()` if the builtin should work through first-class callable syntax or callable aliases.
6. Add optimizer effect modeling in `src/optimize/effects/builtins.rs` when purity, reads, writes, or thrown/fatal behavior matters for DCE/constant propagation.
7. Add the codegen return type to `src/codegen/functions/types/builtins.rs` when local type inference needs to know the result type before the builtin is emitted.
8. Create a new file in `src/codegen/builtins/<category>/` (e.g., `strings/my_func.rs`).
9. Add `mod my_func;` plus dispatcher wiring in the category's `mod.rs`.
10. If the function needs a runtime routine, create it under `src/codegen/runtime/<category>/`.
11. Add module/re-export wiring in the relevant `runtime/<category>/mod.rs`, then call it from the runtime emitter orchestration.
12. Update docs and add codegen/error tests. New PHP-visible builtins should include at least one case-insensitive or namespaced call test when relevant.

Do not stop after wiring only the checker and codegen dispatcher. A builtin is not complete until the catalog, `function_exists()`, case-insensitive lookup, and namespace fallback all see it. New builtins should include at least one case-insensitive or namespaced call test when the feature is PHP-visible.

Leaf builtin/runtime files contain exactly **one emitter function**. Keep dispatcher/re-export files (`mod.rs`) as orchestration-only files, and keep runtime data emission in `src/codegen/runtime/data.rs`.

Do not list every builtin in this guide. `src/types/checker/builtins/catalog.rs` and `src/types/signatures.rs` are the canonical sources; update those instead of maintaining parallel lists.

### Call argument semantics

All function-like call surfaces must share the same argument rules instead of normalizing locally in individual emitters:

- Shared named/positional/spread semantics live in `src/types/call_args.rs` whenever they are not codegen-specific.
- `src/types/call_args.rs` owns the semantic planner (`CallArgPlan` / `plan_call_args`). The checker and codegen should consume that plan; they should not rebuild named-argument matching, duplicate detection, static associative-spread expansion, spread bounds, or the regular/variadic split locally.
- If a codegen surface uses an internal signature with hidden parameters, such as closure captures, pass the caller-visible regular parameter count through `plan_call_args_with_regular_param_count()` instead of letting the planner infer it from the full internal signature.
- Type-checker validation and diagnostic mapping lives in `src/types/checker/functions/call_validation.rs`; it maps planner errors to `CompileError` diagnostics instead of owning the semantic rules.
- `src/codegen/expr/calls/args.rs` is call-argument orchestration: planner consumption, spread checks, and ABI materialization. Source-order named/spread lowering lives in `src/codegen/expr/calls/args/named.rs` and must also consume the shared plan.
- User-defined calls, builtins, and extern calls must use the same named/spread normalization rules before any callee-specific lowering runs.
- PHP call unpacking with static string keys maps to named arguments (`f(...["a" => 1])` behaves like `f(a: 1)`). Static numeric keys remain positional, and duplicate static string keys inside one unpack use PHP's last-wins behavior before planning.
- When adding or extending a builtin, verify `first_class_callable_builtin_sig()` as well as the direct builtin signature so first-class callable syntax and callable aliases stay coherent.
- PHP source evaluation order is distinct from ABI parameter order. Preserve side effects in source order, then materialize arguments in parameter/ABI order; extern calls follow the same rule before C ABI register loading.
- Spread arguments before named arguments must be evaluated once, length/overwrite checks must happen at the PHP-observable point, and later named-argument side effects must not be skipped by early codegen checks. Too-short spreads for required parameters must fail instead of reading past the array payload.
- A positional spread into a variadic callee fills visible regular parameters first; only the remaining tail becomes the variadic array.
- User-defined variadics accept unknown named arguments as string-keyed variadic entries; internal/builtin variadics reject unknown named arguments like PHP internals.
- Ref-like parameters, including mutating builtin parameters, must avoid value-temp preevaluation so the original storage is passed/mutated.
- If hidden named-argument temporaries are introduced, update `src/codegen/functions/locals.rs` and main emission so slots are allocated before frame-size calculation.

### Optimizer and effects

The optimizer assumes side effects are modeled conservatively. When changing calls, operators, expressions, statements, or builtins:

- Update `src/optimize/effects/` if purity, variable reads/writes, call effects, filesystem/runtime state, exceptions/fatals, or by-ref mutation behavior changes.
- Do not mark a call as pure if it can read or write globals, files, environment, runtime heap state, object properties, array contents, argument storage, or can emit visible output.
- Keep constant folding in `src/optimize/fold/` limited to PHP-equivalent results. If PHP behavior is edge-case sensitive, cross-check with `php -r`.
- Add optimizer regression tests under `tests/codegen/optimizer/` when DCE, constant propagation, control-flow pruning, or folding can observe the change.
- Magic constants must be lowered before optimizer passes. Do not introduce optimizer paths that expect raw `ExprKind::MagicConstant`.

### Runtime ownership, GC, and COW

Refcounted runtime values are not plain scalars. When changing arrays, strings, objects, `Mixed`, `Iterable`, call returns, or temporaries:

- Preserve the boxed `Mixed` cell contract: the runtime tag and payload shape must stay consistent across codegen and runtime helpers.
- Respect copy-on-write before mutating arrays or hashes. Use the existing ensure-unique helpers instead of mutating shared storage directly.
- Track whether a value is owned, borrowed, persistent, or a temporary result. Release only values this code path owns.
- Keep cleanup paths balanced across normal returns, early exits, throws/fatals, and control-flow merges.
- Add focused tests in `tests/codegen/runtime_gc/` for ownership, aliasing, cycles, heap debug, stack args, and COW changes.

### File size policy

As a general rule, aim to keep source files under **500 lines of code**. This is a maintainability guideline, not a blind numeric rule.

The real goal is to avoid files that become hard to reason about because they mix multiple responsibilities. In practice:

- **Dispatcher/orchestration files** (`mod.rs`, top-level drivers, large checker/codegen coordinators) should stay slim. If they grow large, split them aggressively.
- **Multi-responsibility files** should be split once they start accumulating unrelated concerns, even if the line count is not yet extreme.
- **Leaf files that implement one cohesive feature** are allowed to exceed 500 lines when splitting them would create artificial fragmentation.

Examples of files that may reasonably stay above the soft limit:

- a single runtime emitter implementing one substantial builtin or runtime routine
- a single compiler pass file that is still clearly about one feature and one code path
- a self-contained parser/lowering/runtime leaf where splitting would only spread one mental model across several tiny files

Examples of files that should usually be split:

- a file that mixes dispatch, validation, data collection, and post-processing
- a file that contains several unrelated builtins or runtime helpers
- a file that acts as a “miscellaneous bucket” for code that did not get a home

So the policy is:

- treat **500 LOC as a warning sign**
- treat **mixed responsibilities** as the real trigger for refactoring
- do **not** split a file that owns one coherent feature just to satisfy the number

In short: prefer **cohesion over mechanical line-count compliance**. A 650-line mono-feature leaf is acceptable; a 350-line multi-purpose orchestrator is already a refactor candidate.

### Rust module preamble policy

Every repo-owned Rust source file (`*.rs`) must start with a module-level Rustdoc preamble before any `use`, `mod`, item, or test helper code. Use `//!` comments so the explanation is attached to the module in rustdoc.

The preamble is mandatory for all new Rust files and must be added or preserved when touching existing Rust files. Release verification should report any Rust file that is missing it.

Standard format:

```rust
//! Purpose:
//! Explain what this file owns in 2-4 lines.
//!
//! Called from:
//! - `crate::path::caller()` or the relevant test/module entry point.
//!
//! Key details:
//! - Important invariants, ordering constraints, ownership/ABI/runtime rules, or coupling.
```

For test files, use the same structure but describe the test surface instead of production callers:

```rust
//! Purpose:
//! Integration or regression tests for the relevant feature area.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Fixture layout, platform assumptions, ignored-test requirements, or why edge cases exist.
```

Keep preambles concise and factual. Do not include refactor history, stale line numbers, or broad architecture prose that belongs in `docs/internals/`.

### Rust function docblock policy

Every explicit Rust function in repo-owned Rust source files must have a concise docblock explaining what that function does. This applies equally to public functions, restricted-visibility functions (`pub(crate)`, `pub(super)`, etc.), private helper functions, impl methods, trait methods, and test functions.

Use `///` Rustdoc comments immediately before the function item or its item attributes. Keep the docblock specific to the function's actual responsibility, inputs, outputs, side effects, ownership/ABI/runtime constraints, and failure behavior when those details matter. Do not use vague filler such as "handles logic" or "processes data".

When documenting test functions, describe the behavior or regression being verified and any important fixture/platform assumptions. For explanatory comments inside a function body, use normal `//` comments, not `///`; Rustdoc comments inside function bodies produce warnings or errors because they do not document an item.

Adding or updating function docblocks must not change code behavior. Do not alter function signatures, visibility, attributes, derives, module declarations, control-flow braces, strings, assembly instructions, or instruction-comment alignment while adding documentation. If a doc-only change causes `cargo check`, `cargo check --tests`, or `git diff --check` to fail, fix the documentation placement or comment style rather than changing code to fit the comment.

### Codegen conventions (target-aware)

- Prefer helpers from `src/codegen/abi/` for registers, stack slots, frame layout, argument materialization, symbol addresses, and calls.
- New feature emitters must support every supported target through `emitter.target` or clearly isolate target-specific code behind existing target helpers with explicit tests and diagnostics.
- Avoid hardcoding ARM64 register names, x86_64 register names, syscall numbers, object formats, or stack alignment rules in shared lowering code.
- Do not add an ARM64-only runtime helper, builtin emitter, ABI path, or ownership cleanup path unless the feature is intentionally target-gated and documented as unsupported elsewhere.
- Test target-sensitive changes on every supported target they can affect. Use the Docker Linux scripts when the change can affect Linux x86_64 or Linux ARM64.

### ARM64 quick reference

- **Integers**: result in `x0`
- **Floats**: result in `d0`
- **Strings**: pointer in `x1`, length in `x2`
- **Function args**: `x0`-`x7` (int = 1 reg, string = 2 regs), `d0`-`d7` (floats)
- **Return value**: same as expression result (`x0`, `d0`, or `x1`/`x2`)
- **Stack frame**: `x29` = frame pointer, `x30` = link register, locals at negative offsets from `x29`
- **ABI helpers**: `src/codegen/abi/` centralizes load/store/write per type
- **Labels**: use `ctx.next_label("prefix")` — global counter prevents collisions across functions
- **Mixed values**: `PhpType::Mixed` is an internal boxed runtime shape used for heterogeneous associative-array values; codegen/runtime must preserve the boxed cell contract instead of treating it like a plain scalar

### Assembly comment policy

**Every `emitter.instruction(...)` call MUST have an inline `//` comment** explaining what the assembly instruction does. This is mandatory — the generated assembly is meant to be read, and every assembly line must be understandable by someone learning how compilers work.

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

**PHP-derived syntax must be 100% compatible with PHP.** When elephc implements a PHP construct (variables, operators, keywords, built-ins), it must behave identically to PHP. This means:

- Variable names, keywords, operators, and built-in function names must match PHP exactly
- Superglobals (`$argc`, `$argv`) must use PHP's syntax (e.g., `$argv[0]`, not `argv(0)`)
- Operator precedence and associativity must match PHP
- String escape sequences must match PHP behavior
- Built-in function signatures must match PHP (argument count, order, types)

When in doubt, test with `php -r '...'` to verify behavior.

**elephc also provides compiler-specific extensions** beyond standard PHP (e.g., `ptr`, `extern`, `buffer<T>`, `packed class`). These features have no PHP equivalent and are not expected to run under the PHP interpreter. They are clearly distinguishable from PHP syntax and exist to enable use cases (FFI, game development, low-level memory access) that PHP cannot address.

## Documentation

The `docs/` directory is the project's complete documentation, organized into three sections:

```
docs/
├── README.md              # Main index
├── getting-started/       # Installation and first program
│   ├── installation.md
│   └── your-first-program.md
├── php/                   # PHP syntax (standard PHP features)
│   ├── types.md
│   ├── operators.md
│   ├── control-structures.md
│   ├── functions.md
│   ├── strings.md
│   ├── arrays.md
│   ├── math.md
│   ├── classes.md
│   ├── namespaces.md
│   ├── magic-constants.md
│   └── system-and-io.md
├── beyond-php/            # Compiler extensions (not valid PHP)
│   ├── pointers.md
│   ├── buffers.md
│   ├── packed-classes.md
│   ├── extern.md
│   └── ifdef.md
└── internals/             # Compiler internals
    ├── what-is-a-compiler.md
    ├── how-elephc-works.md
    ├── the-lexer.md
    ├── the-parser.md
    ├── the-type-checker.md
    ├── the-optimizer.md
    ├── the-codegen.md
    ├── the-runtime.md
    ├── memory-model.md
    ├── architecture.md
    ├── arm64-assembly.md
    └── arm64-instructions.md
```

### Astro compatibility

All docs files are Markdown with YAML frontmatter compatible with Astro content collections. Every `.md` file **must** have this frontmatter format:

```yaml
---
title: "Page Title"
description: "One-line description of the page."
sidebar:
  order: N
---
```

- `title` replaces the `# Heading` — do **not** add a top-level `# Title` in the body (Astro renders it from frontmatter)
- `sidebar.order` controls page ordering within its section
- No navigation links (`[← Back]`, `Next:`, etc.) — Astro handles navigation
- Use standard Markdown (CommonMark). No custom shortcodes or Astro components inside docs

### Keeping docs up to date

**Documentation must be kept up to date.** When adding a new feature:

1. **PHP syntax feature** (operator, built-in, statement, etc.) → update the relevant page in `docs/php/`. Add the function signature, parameters, return type, and a short example.
2. **Compiler extension** (pointer, buffer, extern, ifdef) → update the relevant page in `docs/beyond-php/`.
3. **Compiler internals change** (pipeline, type checker, optimizer, codegen, runtime, ABI, memory model) → update the relevant page in `docs/internals/`.
4. If a feature was previously listed as "not supported", remove that note.
5. If there are known incompatibilities with PHP, document them in `docs/php/types.md` (incompatibilities section).
6. Update `docs/README.md` index if adding a new page.

## Roadmap management

`ROADMAP.md` tracks all planned and completed work, organized by version.

- **Never remove completed items** from a version section. Mark them as `[x]` and leave them under the version they belong to. This preserves the history of what was delivered in each release.
- New work items go under the appropriate future version.
- When all items in a version are completed, the version is considered done — do not move items elsewhere.

## Conventions

- No `Co-Authored-By` lines in commits
- Use commit message prefixes such as `feat:`, `fix:`, `chore:`, `docs:`, `refactor:`, or `test:`
- Keep commit messages concise
- Run the pre-commit verification above before committing code changes — all tests must pass
- Zero compiler warnings policy (`cargo build` must be clean)
- Never run `cargo fmt` in this repo. Use targeted manual edits only; global formatting creates noisy churn here.
