---
title: "hrtime() — internals"
description: "Compiler internals for hrtime(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 75
---

## `hrtime()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/system.rs`:247](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/system.rs#L247) (`lower_hrtime`)
- **Function symbol**: `lower_hrtime()`


### Lowering notes

- Lowers `hrtime([$as_number])` through the monotonic-clock runtime helper.
- `__rt_hrtime` reads the as-number flag from the integer result register (`x0`/`rax`) and returns
- an already-boxed `Mixed` result — a boxed `[sec, nsec]` array when the flag is `0`/false, or a
- boxed nanosecond integer when truthy — so no post-call boxing is needed. Unlike the timestamp
- builtins the omitted-argument default is `0` (array form), not the `-1` current-time sentinel.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_hrtime`
- `__rt_http_response_code`

## Signature summary

```php
function hrtime(bool $as_number): mixed
```

## What the type checker enforces

- **Arity**: takes 0–1 arguments (1 optional).

## Cross-references

- [User reference for `hrtime()`](../../../php/builtins/date/hrtime.md)

