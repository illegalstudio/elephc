---
name: doc-update
description: Audit and update the docs/ wiki to match the current codebase — checks all doc files against source code and fixes any mismatches.
user-invocable: true
---

# Documentation Update

You are a documentation maintainer for the elephc PHP-to-ARM64 compiler. Your job is to audit every file in `docs/` against the actual codebase and fix any mismatches, missing features, or outdated information.

## What to check

For each doc file, compare against the authoritative source files listed below. The codebase is always the source of truth — docs must reflect reality, not the other way around.

### 1. `docs/architecture.md` — Module map & file counts

**Source of truth:** actual file listing in `src/`

- Count files in each directory: `src/codegen/builtins/{strings,arrays,math,types,io,system}/`, `src/codegen/runtime/{strings,arrays,io,system}/`
- Update the module map tree to match. File counts in parentheses (e.g., `(57 files)`) must be accurate.
- If a new category directory exists (e.g., `builtins/io/`), add it to the tree.
- Check the ARM64 calling conventions table against `src/codegen/abi.rs`.
- Check the runtime memory layout section against `src/codegen/runtime/mod.rs` (`emit_runtime_data()`).

### 2. `docs/language-reference.md` — Supported features

**Source of truth:** `src/types/mod.rs` (PhpType enum), `src/types/checker/builtins.rs` (all builtin signatures), `src/parser/ast.rs` (ExprKind, StmtKind)

- Every type in `PhpType` must be in the Data Types table.
- Every builtin function matched in `src/codegen/builtins/*/mod.rs` must appear in the Built-in Functions tables with correct signature.
- Every `StmtKind` variant must be covered in Control Structures or Statements.
- Every `ExprKind` variant must be covered somewhere (Operators, Expressions, etc.).
- "Not supported yet" notes must not refer to features that have been implemented.
- The Limitations sections must be accurate.

### 3. `docs/the-lexer.md` — Token types

**Source of truth:** `src/lexer/token.rs`

- Every variant in the `Token` enum should appear in the token tables.
- New keywords, operators, or structural tokens must be documented.

### 4. `docs/the-parser.md` — AST nodes

**Source of truth:** `src/parser/ast.rs`

- The Expressions table must list every `ExprKind` variant.
- The Statements table must list every `StmtKind` variant.
- The `BinOp` enum must match the documented operators.
- The `Foreach` struct fields must be accurate (check for `key_var`).
- The Pratt parser binding power table should match `src/parser/expr.rs` `infix_bp()`.

### 5. `docs/the-type-checker.md` — Type system

**Source of truth:** `src/types/mod.rs`

- The `PhpType` enum shown in the doc must match the actual enum exactly.
- Type compatibility rules should match the logic in `src/types/checker/mod.rs`.
- The Literals type table should cover all types including arrays and associative arrays.

### 6. `docs/the-codegen.md` — Code generation

**Source of truth:** `src/codegen/expr.rs`, `src/codegen/stmt.rs`

- All expression types should have codegen explanations (or at least be mentioned).
- Associative array codegen should be documented if `ArrayLiteralAssoc` exists in ast.rs.
- Switch/match codegen should be documented if `Switch`/`Match` exist in ast.rs.
- The register convention table must match `src/codegen/abi.rs`.

### 7. `docs/the-runtime.md` — Runtime routines

**Source of truth:** `src/codegen/runtime/mod.rs` (`emit_runtime()` function)

- Every `bl __rt_*` call in `emit_runtime()` should appear in the runtime docs.
- Group by category: strings, arrays (core + hash table + manipulation), I/O, system.
- Hash table routines (`__rt_hash_*`) must be documented.
- I/O routines must have their own section.
- The "How routines are emitted" section should mention all categories.

### 8. `docs/memory-model.md` — Memory layout

**Source of truth:** `src/codegen/runtime/mod.rs` (`emit_runtime_data()`), `src/codegen/runtime/arrays/hash_new.rs`

- The memory regions diagram must include all `.comm` buffers declared in `emit_runtime_data()`.
- If hash tables exist, their memory layout (header + entry structure) must be documented.
- The memory limits table must include all buffers.

### 9. `docs/arm64-assembly.md` and `docs/arm64-instructions.md`

These rarely change. Only update if new instruction patterns are used in codegen (check for new ARM64 mnemonics in `src/codegen/`).

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
- After updating, verify no broken cross-references by checking that all `](*.md)` links point to existing files.

## Output

After updating, provide a summary of changes made:

```
## Documentation Update Summary

### Files updated
- `docs/architecture.md` — updated file counts, added I/O section
- `docs/the-runtime.md` — added 15 missing routines, added I/O section
- ...

### Files unchanged
- `docs/arm64-instructions.md` — already up to date
- ...
```
