---
title: "http_response_code() — internals"
description: "Compiler internals for http_response_code(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 293
---

## `http_response_code()` — internals

## Where it lives

- **Signature**: [`src/builtins/system/http_response_code.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/system/http_response_code.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/system.rs`:264](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/system.rs#L264) (`lower_http_response_code`)
- **Function symbol**: `lower_http_response_code()`


### Lowering notes

- Lowers `http_response_code([$code])` to `__rt_http_response_code`. The code (or
- 0 = "read current" when omitted) goes into the first integer argument register;
- the routine returns the resulting status as an int. PHP semantics (read vs set,
- return-previous) live in the bridge's `elephc_web_set_status`.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_header`
- `__rt_http_response_code`

## Signature summary

```php
function http_response_code(int $response_code = 0): int
```

## What the type checker enforces

- **Arity**: takes 0–1 arguments (1 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/time/http_response_code.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/time/http_response_code.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `http_response_code()`](../../../php/builtins/misc/http_response_code.md)
