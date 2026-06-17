---
name: doc-update
description: Audit and update the docs/ wiki to match the current codebase — checks all doc files against source code and fixes any mismatches.
user-invocable: true
---

# Documentation Update

You are a documentation maintainer for the elephc PHP-to-native compiler. Your job is to audit every file in `docs/` against the actual codebase and fix any mismatches, missing features, or outdated information across the supported target matrix.

## What to check

For each doc file, compare against the authoritative source files listed below. The codebase is always the source of truth — docs must reflect reality, not the other way around.

### 1. Current docs tree and Astro metadata

**Source of truth:** actual file listing in `docs/`

The current documentation tree is:

- `docs/README.md`
- `docs/getting-started/`
- `docs/how-to/`
- `docs/compiling/` — the compiler CLI (flags, env vars) and the full compilation process
- `docs/php/`
- `docs/beyond-php/`
- `docs/internals/`

Every `.md` file must have YAML frontmatter with `title`, `description`, and `sidebar.order`. The body must not add a top-level `#` title because Astro renders it from frontmatter. Do not add hand-written previous/next navigation links. When adding a new page, also add it to the `docs/README.md` index.

### 2. `docs/internals/architecture.md` — Module map & file counts

**Source of truth:** actual file listing in `src/`

- Count files in each directory: `src/codegen/builtins/{strings,arrays,math,types,io,system,pointers}/`, `src/codegen/runtime/`, and runtime subdirectories including `arrays`, `strings`, `io`, `system`, `exceptions`, `buffers`, `pointers`, and `fibers`.
- Update the module map tree to match. File counts in parentheses (e.g., `(57 files)`) must be accurate.
- If a new category directory exists (e.g., `builtins/io/`), add it to the tree.
- Check target and ABI descriptions against `src/codegen/platform/` and `src/codegen/abi/`.
- Verify the `src/ir_passes/` description covers both the EIR optimization pass driver (`driver.rs`, identity folding, …) and linear-scan register allocation, not register allocation alone.
- Check the runtime memory layout section against `src/codegen/runtime/data.rs`.

### 3. `docs/php/*.md` and `docs/beyond-php/*.md` — Supported features

**Source of truth:** `src/types/model.rs` (PhpType enum), `src/types/signatures.rs`, `src/types/checker/builtins/catalog.rs` (canonical builtin registry), `src/codegen_ir/lower_inst/builtins/` (active EIR lowering), `src/parser/ast.rs` (ExprKind, StmtKind). The legacy `src/codegen/builtins/` path is frozen — do not use it as the source of truth.

- Every type in `PhpType` must be documented in `docs/php/types.md` or the relevant extension page.
- Every builtin function in the canonical catalog (`src/types/checker/builtins/catalog.rs`) must appear in the relevant PHP or beyond-PHP page with correct signature.
- Every `StmtKind` variant must be covered in `docs/php/control-structures.md`, `docs/php/functions.md`, `docs/php/classes.md`, `docs/php/namespaces.md`, or the relevant extension page.
- Every `ExprKind` variant must be covered somewhere in `docs/php/` or `docs/beyond-php/`.
- "Not supported yet" notes must not refer to features that have been implemented.
- The Limitations sections must be accurate.

### 4. `docs/internals/the-lexer.md` — Token types

**Source of truth:** `src/lexer/token.rs`

- Every variant in the `Token` enum should appear in the token tables.
- New keywords, operators, or structural tokens must be documented.

### 5. `docs/internals/the-parser.md` — AST nodes

**Source of truth:** `src/parser/ast.rs`, `src/parser/expr/pratt.rs`

- The Expressions table must list every `ExprKind` variant.
- The Statements table must list every `StmtKind` variant.
- The `BinOp` enum must match the documented operators.
- The `Foreach` struct fields must be accurate (check for `key_var`).
- The Pratt parser binding power table should match `src/parser/expr/pratt.rs` `infix_bp()`.

### 6. `docs/internals/the-type-checker.md` — Type system

**Source of truth:** `src/types/model.rs`, `src/types/checker/`, `src/types/signatures.rs`

- The `PhpType` enum shown in the doc must match the actual enum exactly.
- Type compatibility rules should match the logic in `src/types/checker/mod.rs`.
- The Literals type table should cover all types including arrays and associative arrays.

### 7. `docs/internals/the-codegen.md` — Code generation

**Source of truth:** `src/codegen/expr.rs`, `src/codegen/expr/`, `src/codegen/stmt.rs`, `src/codegen/stmt/`, `src/codegen/abi/`, `src/codegen/functions/`

- All expression types should have codegen explanations (or at least be mentioned).
- Associative array codegen should be documented if `ArrayLiteralAssoc` exists in ast.rs.
- Switch/match codegen should be documented if `Switch`/`Match` exist in ast.rs.
- Target-specific ABI descriptions must match `src/codegen/abi/`.

### 8. `docs/internals/the-runtime.md` — Runtime routines

**Source of truth:** `src/codegen/runtime/emitters.rs` (the target-aware `emit_runtime()` entry point) and `src/codegen/runtime/`

- Every runtime helper emitted by `emit_runtime()` should appear in the runtime docs.
- Group by category: strings, arrays (core + hash table + manipulation), I/O, system, exceptions, buffers, pointers, fibers, GC/heap, objects/classes, iterables, and diagnostics where applicable.
- Hash table routines (`__rt_hash_*`) must be documented.
- I/O routines must have their own section.
- The "How routines are emitted" section should mention all categories.

### 9. `docs/internals/memory-model.md` — Memory layout

**Source of truth:** `src/codegen/runtime/data.rs`, `src/codegen/runtime/arrays/hash_new.rs`, `src/codegen/runtime/fibers/`

- The memory regions diagram must include all `.comm` buffers declared by `src/codegen/runtime/data.rs`.
- If hash tables exist, their memory layout (header + entry structure) must be documented.
- The memory limits table must include all buffers.

### 10. `docs/internals/arm64-assembly.md` and `docs/internals/arm64-instructions.md`

These rarely change. Only update if new instruction patterns are used in codegen (check for new ARM64 mnemonics in `src/codegen/`).

### 11. `docs/compiling/*.md` — Compiler CLI and the compilation process

**Source of truth:** `src/cli.rs` (flag parsing, defaults, env-var overrides), `src/pipeline.rs` (phase order and timing labels), `src/codegen/platform/` (targets), `src/ir_passes/` (EIR optimization passes), `README.md` (Usage section).

- `docs/compiling/cli-reference.md` must document EVERY flag and environment variable parsed in `src/cli.rs`, with the correct accepted values and default. A new, renamed, or removed flag with no matching entry is a mismatch to fix.
- `docs/compiling/compilation-pipeline.md` phase list and labels must match the `timings.record_since("…")` labels and ordering in `src/pipeline.rs`.
- `docs/compiling/targets.md` must match the supported target matrix and accepted spellings in `src/codegen/platform/`.
- `docs/compiling/optimization.md` must match the optimization controls: `--ir-opt`/`--no-ir-opt` (the EIR pass driver in `src/ir_passes/`), `--regalloc`, and `--null-repr`.
- Keep user-facing flag examples in `README.md` consistent with this section.

## How to update

1. **Read the source file** to get the current truth.
2. **Read the doc file** to see what it says.
3. **Diff** mentally — find mismatches.
4. **Edit** the doc file to match reality. Keep the existing style and tone.
5. **Do not remove content** that is still accurate. Only add/update/fix.

## Rules

- All documentation is written in **English**.
- Keep the existing structure and formatting style of each doc file.
- File counts must be exact — count with `ls dir/*.rs | wc -l`.
- When adding new sections, place them logically (e.g., hash tables near arrays, I/O after arrays).
- Cross-links between docs (e.g., `[The Runtime](the-runtime.md)`) should be added when referencing another page.
- Use tables for lists of items (routines, types, tokens). Use prose for explanations.
- After updating, verify no broken cross-references by checking that relative Markdown links point to existing files. Ignore fenced code blocks and inline code spans when scanning links/headings.

## Output

After updating, provide a summary of changes made:

```
## Documentation Update Summary

### Files updated
- `docs/internals/architecture.md` — updated file counts, added I/O section
- `docs/internals/the-runtime.md` — added 15 missing routines, added I/O section
- ...

### Files unchanged
- `docs/arm64-instructions.md` — already up to date
- ...
```
