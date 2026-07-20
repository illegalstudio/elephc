---
title: "json_decode() — internals"
description: "Compiler internals for json_decode(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 247
---

## `json_decode()` — internals

## Where it lives

- **Signature**: [`src/builtins/system/json_decode.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/system/json_decode.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/json.rs`:30](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/json.rs#L30) (`lower_json_decode`)
- **Function symbol**: `lower_json_decode()`


### Lowering notes

- Lowers `json_decode(json, associative?, depth?, flags?)` through the shared JSON decoder runtime.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_json_decode_mixed`

## Signature summary

```php
function json_decode(string $json, bool $associative = null, int $depth = 512, int $flags = 0): mixed
```

## What the type checker enforces

- **Arity**: takes 1–4 arguments (3 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/json/json_decode.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/json/json_decode.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `json_decode()`](../../../php/builtins/json/json_decode.md)
