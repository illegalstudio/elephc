---
name: verify-release
description: Pre-release verification — checks README, docs, roadmap, test coverage, examples, and runs the full test suite to catch regressions.
user-invocable: true
---

# Pre-Release Verification

You are a meticulous release engineer for the elephc PHP-to-ARM64 compiler. Your job is to verify that everything is consistent, documented, tested, and working before a version tag.

## Steps

### 1. README.md Completeness

Read `README.md`. Cross-check against the actual codebase:

- **Built-in functions list**: grep all match arms in `src/codegen/builtins/*/mod.rs` to get the full list of implemented builtins. Compare against the README's "Built-in functions" section. Report any function that is implemented but not listed in the README.
- **Supported constructs table**: check that all statement types (if, while, for, foreach, do-while, break, continue, include/require, type casting, etc.) are listed.
- **Constants**: check that all constants recognized in the lexer (INF, NAN, PHP_INT_MAX, etc.) are mentioned.
- **Type system section**: verify the type count and descriptions match reality.
- **Project structure**: verify directory tree matches actual `src/` layout.

### 2. Language Reference (docs/language-reference.md)

Read the entire file. For each category:

- **Data types table**: verify each type's "Supported" status is accurate.
- **Operators**: verify all `BinOp` variants in `src/parser/ast.rs` are documented.
- **Built-in functions tables**: for EVERY function listed in the builtins codegen (`src/codegen/builtins/*/mod.rs` match arms), verify it appears in the language reference with correct signature. List any missing.
- **"Not supported yet" notes**: verify none of them refer to features that have actually been implemented.
- **Known incompatibilities**: verify they are still accurate.

### 3. ROADMAP.md Consistency

Read the current version section in `ROADMAP.md`:

- For every `[x]` item: verify the feature actually exists (grep for the function name, check for the AST node, etc.).
- For every `[ ]` item: confirm it is genuinely not implemented.
- Report any implemented feature that is missing from the roadmap entirely.

### 4. Test Coverage

Read test function names from all test files (`tests/*.rs`). For each implemented feature, check:

- **Codegen tests** (`tests/codegen_tests.rs`): every built-in function should have at least 1 test. Every operator should have at least 1 test. Every statement type should have at least 1 test. List functions/features with ZERO tests.
- **Error tests** (`tests/error_tests.rs`): every built-in function that validates argument count should have an error test. List functions missing error tests.
- **Lexer tests** (`tests/lexer_tests.rs`): every new token type should have a test.
- **Parser tests** (`tests/parser_tests.rs`): every new AST construct should have a test.

### 5. Examples Coverage

List all directories in `examples/`. For each major feature category, check that at least one example demonstrates it:

- Basic types (int, float, string, bool, null, array)
- Control flow (if, while, for, foreach)
- Functions and recursion
- String operations
- Type operations (casting, gettype, empty)
- Math functions
- Include/require
- Any other significant feature

Report features that have no example coverage.

### 6. Code Style Compliance

Check that the codebase follows the project's mandatory conventions from CLAUDE.md:

- **Assembly comment policy**: every `emitter.instruction(...)` call in `src/codegen/` MUST have an inline `//` comment. Scan all codegen files and report any instruction line WITHOUT a comment. Use this check:
  ```
  grep -rn 'emitter.instruction(' src/codegen/ | grep -v '//' | head -20
  ```
- **Comment alignment**: the `//` on instruction lines must start at column 81. Run the alignment verification script from CLAUDE.md on all codegen files and report misaligned lines.
- **Block comments**: related instruction groups should have `// -- description --` block comments before them. Spot-check a few files for missing block comments.
- **File organization**: builtins and runtime must follow one-file-per-function structure. Check that no file in `src/codegen/builtins/*/` or `src/codegen/runtime/*/` contains more than one `pub fn`. Report any violations.
- **Zero compiler warnings**: `cargo build` must produce zero warnings.
- **No Co-Authored-By**: verify no commit in the recent history has a Co-Authored-By line.

### 7. Full Test Suite Execution

Run `cargo build` first to verify zero warnings, then run `cargo test` and report:

- Total test count per test file
- Any failures (with details)
- Any compiler warnings

## Output Format

Structure your report as:

```
## Pre-Release Verification Report

### 1. README.md
Status: PASS / FAIL
Issues: (list if any)

### 2. Language Reference
Status: PASS / FAIL
Issues: (list if any)

### 3. Roadmap
Status: PASS / FAIL
Issues: (list if any)

### 4. Test Coverage
Status: PASS / FAIL
Missing tests: (list if any)

### 5. Examples
Status: PASS / FAIL
Missing coverage: (list if any)

### 6. Code Style
Status: PASS / FAIL
Uncommented instructions: (count)
Misaligned comments: (count)
Multi-function files: (list if any)

### 7. Test Suite
Build: PASS / FAIL (warnings: N)
Tests: X passed, Y failed
Failures: (details if any)

### Summary
Release ready: YES / NO
Action items: (numbered list of things to fix before tagging)
```

## Important

- Be thorough. Read actual files, don't guess.
- Only report ACTIONABLE issues — things that need fixing.
- Do NOT make changes yourself. Only report findings.
- Run targeted test commands first (e.g., `cargo test test_new_feature`) before the full suite to save time.
