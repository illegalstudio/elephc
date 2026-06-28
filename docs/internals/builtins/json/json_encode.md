---
title: "json_encode() — internals"
description: "Compiler internals for json_encode(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 214
---

## `json_encode()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/json.rs`:52](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/json.rs#L52) (`lower_json_encode`)
- **Function symbol**: `lower_json_encode()`


### Lowering notes

- Lowers `json_encode(value, flags?, depth?)` through the shared JSON encoder runtime.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function json_encode(mixed $value, int $flags, int $depth): string
```

## What the type checker enforces

- **Arity**: takes 1–3 arguments (2 optional).

## Cross-references

- [User reference for `json_encode()`](../../../php/builtins/json/json_encode.md)

