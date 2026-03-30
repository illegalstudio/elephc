---
name: analyze-using-rustrover
description: Run RustRover IDE inspections on the codebase and produce a structured report of errors, warnings, and code quality issues.
user-invocable: true
---

# Analyze Using RustRover

You are a code quality analyst. Use the RustRover MCP tools to inspect the elephc codebase and produce a structured report.

## Scope

By default, analyze **all** `.rs` source files in the project. If the user specifies a file or directory, limit the analysis to that scope.

## Steps

### 1. Discover all source files

Use the Glob tool to find every `.rs` file in the project:
```
pattern: "src/**/*.rs"
```

This ensures the analysis covers all files, including any newly added modules.

### 2. Errors pass

Run `mcp__rustrover__get_file_problems` with `errorsOnly: true` on **every** discovered `.rs` file. Always pass `projectPath` set to the current working directory.

Batch calls in parallel (up to 6 per message) to save time. Use relative paths from the project root (e.g., `src/codegen/mod.rs`).

Collect all errors.

### 3. Warnings pass

Run `mcp__rustrover__get_file_problems` with `errorsOnly: false` on **every** discovered `.rs` file.

Batch calls in parallel. Collect all warnings.

Categorize warnings into:
- **Duplicated code**: note the file, line, and fragment length
- **Unnecessary path prefixes**: note the file and the prefix that could be simplified
- **Other warnings**: anything else RustRover flags

### 4. Verify with compiler

Run these commands to cross-validate RustRover findings:
```bash
cargo build 2>&1 | tail -5
cargo clippy 2>&1 | grep "warning:" | grep -v "generated"
```

This confirms whether RustRover errors are real or false positives. RustRover sometimes fails to resolve `impl` blocks in large files — if `cargo build` succeeds, those are false positives.

### 5. Also analyze tests and main

Include `src/main.rs` and all files in `tests/` in the analysis:
```
pattern: "tests/**/*.rs"
```

## Output Format

```
## RustRover Inspection Report

**Files analyzed:** N

### Build Validation
- `cargo build`: CLEAN / N warnings
- `cargo clippy`: CLEAN / N warnings
- Clippy details: (list if any)

### Real Errors
(Errors confirmed by both RustRover and cargo build)

| File | Line | Severity | Description |
|------|------|----------|-------------|
| ... | ... | ERROR | ... |

If none: "No real errors found."

### False Positives
(RustRover errors that cargo build does not reproduce)

| File | Count | Likely cause |
|------|-------|-------------|
| ... | ... | Large file / impl block resolution |

### Code Quality Warnings

#### Duplicated Code
| File | Line | Fragment length | Description |
|------|------|-----------------|-------------|
| ... | ... | N lines | ... |

Summary: N duplicated fragments across M files. Top candidates for extraction: (list the most impactful)

#### Unnecessary Path Prefixes
| File | Line | Current | Suggested |
|------|------|---------|-----------|
| ... | ... | `self::functions::` | `functions::` |

#### Other Warnings
| File | Line | Description |
|------|------|-------------|
| ... | ... | ... |

### Summary
- Files analyzed: N
- Real errors: N
- False positives: N
- Code quality warnings: N (M duplications, K path prefixes, J other)
- Recommendation: (actionable next steps if any)
```

## Important

- Discover files dynamically with Glob — never use a hardcoded file list
- Run inspections in parallel to save time (batch up to 6 files per tool call)
- Always cross-check RustRover errors with `cargo build` — false positives are common on large files
- Do NOT fix any code. Only report findings.
- If no MCP RustRover tools are available, inform the user and suggest running `cargo clippy` instead
