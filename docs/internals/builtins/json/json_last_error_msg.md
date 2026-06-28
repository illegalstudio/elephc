---
title: "json_last_error_msg() — internals"
description: "Compiler internals for json_last_error_msg(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 216
---

## `json_last_error_msg()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/json.rs`:85](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/json.rs#L85) (`lower_json_last_error_msg`)
- **Function symbol**: `lower_json_last_error_msg()`


### Lowering notes

- Lowers `json_last_error_msg()` through the runtime message lookup table.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_json_last_error_msg`
- `__rt_json_validate`

## Signature summary

```php
function json_last_error_msg(): string
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Cross-references

- [User reference for `json_last_error_msg()`](../../../php/builtins/json/json_last_error_msg.md)

