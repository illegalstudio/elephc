---
title: "Global State Audit"
description: "Mutable global symbol classification, the audit gate, and centralized symbol addressing."
sidebar:
  order: 14
---

The elephc runtime emits mutable global state as `.comm` and `.globl` symbols
in the assembled binary. As the compiler grows, the set of mutable globals
expands, and untracked additions can introduce subtle correctness issues
(e.g. a mutable global that should be per-request state but is accidentally
shared across all requests in a long-running process).

The global-state audit is a hygiene tool that classifies every mutable global
symbol into one of five categories and enforces a versioned baseline so that
new symbols must be explicitly classified before they land.

## Classification scheme

| Class | Meaning | Example |
|-------|---------|---------|
| `CONST` | Read-only data: string literals, lookup tables, error messages. Never mutated after link. | `_b64_encode_tbl`, `_heap_err_msg` |
| `RO_BOOT` | Written once at process boot, then read-only for the process lifetime. | `_class_vtable_ptrs`, `_global_argc` |
| `SHARED_LOCK` | Mutable across requests/workers; requires explicit locking. | `SERVED`, `CHILD_DISPATCH_CHAN` |
| `TLS` | Mutable per-thread/per-request state. | `_heap_buf`, `_gc_live`, `_exc_value` |
| `UNKNOWN` | Not yet classified. The audit gate blocks landing until classified. | (should be zero in a clean baseline) |

## The audit crate

The `elephc-globals-audit` crate (in `crates/elephc-globals-audit/`) provides:

- **Extraction**: walks `src/codegen/runtime/data/`, `src/codegen/data_section.rs`,
  `src/codegen/runtime/`, and `crates/elephc-*/src/` for `.comm`, `.globl`,
  and `static mut` declarations.
- **Classification**: heuristic classifier assigns one of the five classes
  based on symbol naming conventions (prefixes, suffixes, patterns).
- **Baseline tables**: per-target TOML files in `baseline/` record the
  classified symbol set. The baseline is the source of truth.
- **`--check` mode**: compares the live extraction against the committed
  baseline. Fails (exit 1) if:
  - a new symbol appears that is not in the baseline (`new_symbols`)
  - a symbol's classification changes from the baseline (`changed`)
  - a baseline symbol is no longer emitted (`removed`)

### Running the audit

```bash
# Check the working tree against the committed baseline (host target):
cargo run -p elephc-globals-audit -- --check

# Regenerate the baseline seed (prints TOML to stdout for review):
cargo run -p elephc-globals-audit -- --generate > baseline/macos-aarch64.toml

# Audit a specific target:
cargo run -p elephc-globals-audit -- --check --target linux-aarch64
```

### Bumping the baseline

When a new mutable global is intentionally added:

1. Run `--generate` to get a fresh classification seed.
2. Review the `UNKNOWN` entries (if any) and assign the correct class.
3. Replace the relevant `baseline/<target>.toml` file.
4. Run `--check` to confirm the baseline is consistent.
5. Commit the baseline bump alongside the code change that introduced the
   new symbol.

The baseline files are target-specific (`macos-aarch64.toml`,
`linux-aarch64.toml`, `linux-x86_64.toml`). Source-level extraction is
target-independent, so the three files are currently identical. Emitted-symbol
verification per target (via `objdump` of `--emit-asm` output) is CI-deferred.

## Centralized symbol addressing

All mutable-symbol address loads in the compiler go through a single seam:
`emit_symbol_address` in `src/codegen/abi/symbols.rs`. This helper emits the
target-appropriate addressing pattern:

- **AArch64 (non-PIC)**: `adrp dest, sym@PAGE` + `add dest, dest, sym@PAGEOFF`
- **AArch64 (PIC, cdylib)**: GOT-relative load via `adrp` + `ldr`
- **x86_64**: `lea dest, [rip+sym]`

A paged-load variant (`emit_load_symbol_to_reg_via_page`) handles the
`adrp + ldr [reg, :lo12:sym]` pattern for loading a value directly from a
symbol address.

### Why centralization matters

Before centralization, ~39 sites across `src/codegen/` and `src/codegen_ir/`
emitted raw `emitter.adrp(...)` calls with hand-written `add`/`ldr` follow-ups.
This made it difficult to:

- audit which symbols were being addressed and how
- switch the addressing mode uniformly (e.g. for PIC or future TLS migration)
- verify that all mutable-symbol loads used a consistent relocation pattern

After centralization, every mutable-symbol address load goes through one of
two helpers, and an audit test (`no_raw_adrp_on_mutable_symbols_outside_symbols_rs`)
forbids new raw `emitter.adrp()` calls outside `src/codegen/abi/symbols.rs`.

### Verification

The centralization was verified byte-identical on `macos-aarch64` via
`objdump -dr` inspection of the runtime object: the relocated `adrp+add` pairs
for previously-inline sites (e.g. `_strtotime_clock`, `_php_tz_save`) now
carry `ARM64_RELOC_PAGE21` + `ARM64_RELOC_PAGEOFF12` relocations identical to
the pre-centralization pattern. Focused codegen tests (strtotime, date,
fputcsv, callable, objects, math, conversions) pass.

## Addressing-mode microbench

A microbench harness in `crates/elephc-globals-audit/microbench/` measures the
relative cost of three addressing mechanisms for mutable global access in hot
loops: baseline (`adrp+add` / RIP-relative), native TLS (`__thread`), and a
reserved context register (`x28` / `r15`). Results are in
`crates/elephc-globals-audit/microbench-results.md`.

The microbench provides **data only** — it does not select a mechanism. Key
finding: on all three targets, the compiler hoists addressing setup outside
hot loops, making baseline and native TLS have identical per-iteration
instruction counts.