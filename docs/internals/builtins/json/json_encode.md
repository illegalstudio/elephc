---
title: "json_encode() — internals"
description: "Compiler internals for json_encode(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 248
---

## `json_encode()` — internals

## Where it lives

- **Signature**: [`src/builtins/system/json_encode.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/system/json_encode.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/json.rs`:52](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/json.rs#L52) (`lower_json_encode`)
- **Function symbol**: `lower_json_encode()`


### Lowering notes

- Lowers `json_encode(value, flags?, depth?)` through the shared JSON encoder runtime.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function json_encode(mixed $value, int $flags = 0, int $depth = 512): string
```

## What the type checker enforces

- **Arity**: takes 1–3 arguments (2 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/json/json_encode.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/json/json_encode.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `json_encode()`](../../../php/builtins/json/json_encode.md)
