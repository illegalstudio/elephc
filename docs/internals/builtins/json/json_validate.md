---
title: "json_validate() — internals"
description: "Compiler internals for json_validate(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 217
---

## `json_validate()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/json.rs`:95](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/json.rs#L95) (`lower_json_validate`)
- **Function symbol**: `lower_json_validate()`


### Lowering notes

- Lowers `json_validate(json, depth?, flags?)` into the shared validator runtime.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_json_validate`

## Signature summary

```php
function json_validate(string $json, int $depth, int $flags): bool
```

## What the type checker enforces

- **Arity**: takes 1–3 arguments (2 optional).

## Cross-references

- [User reference for `json_validate()`](../../../php/builtins/json/json_validate.md)

