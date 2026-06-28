---
title: "json_last_error() — internals"
description: "Compiler internals for json_last_error(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 215
---

## `json_last_error()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/json.rs`:70](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/json.rs#L70) (`lower_json_last_error`)
- **Function symbol**: `lower_json_last_error()`


### Lowering notes

- Lowers `json_last_error()` by reading the shared runtime error-code symbol.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_json_last_error_msg`

## Signature summary

```php
function json_last_error(): int
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Cross-references

- [User reference for `json_last_error()`](../../../php/builtins/json/json_last_error.md)

