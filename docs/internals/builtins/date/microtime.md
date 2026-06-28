---
title: "microtime() — internals"
description: "Compiler internals for microtime(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 77
---

## `microtime()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/system.rs`:111](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/system.rs#L111) (`lower_microtime`)
- **Function symbol**: `lower_microtime()`


### Lowering notes

- Lowers `microtime()` / `microtime(true)` / `microtime(false)` / `microtime($flag)`.
- Dispatch is driven by the arg-aware result type set in `ir_lower` (see
- `call_return_type_for_args` and the `microtime` fallback in `call_return_type`):
- `Float` (literal `true`) calls the existing `__rt_microtime` float helper; `Str`
- (omitted / literal `false`) calls `__rt_microtime_str`, which builds the
- "0.NNNNNNNN sec" string on the stack and persists it; `Mixed` (non-literal flag)
- marshals the flag and calls `__rt_microtime_mixed`, which branches at runtime and
- boxes either the string or the float.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_microtime`
- `__rt_microtime_mixed`
- `__rt_microtime_str`

## Signature summary

```php
function microtime(bool $as_float): int
```

## What the type checker enforces

- **Arity**: takes 0–1 arguments (1 optional).

## Cross-references

- [User reference for `microtime()`](../../../php/builtins/date/microtime.md)

