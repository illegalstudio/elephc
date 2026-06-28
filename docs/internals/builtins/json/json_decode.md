---
title: "json_decode() — internals"
description: "Compiler internals for json_decode(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 213
---

## `json_decode()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/json.rs`:30](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/json.rs#L30) (`lower_json_decode`)
- **Function symbol**: `lower_json_decode()`


### Lowering notes

- Lowers `json_decode(json, associative?, depth?, flags?)` through the shared JSON decoder runtime.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_json_decode_mixed`

## Signature summary

```php
function json_decode(string $json, bool $associative, int $depth, int $flags): mixed
```

## What the type checker enforces

- **Arity**: takes 1–4 arguments (3 optional).

## Cross-references

- [User reference for `json_decode()`](../../../php/builtins/json/json_decode.md)

