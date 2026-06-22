---
title: "implode() — internals"
description: "Compiler internals for implode(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 347
---

## `implode()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/strings.rs`:192](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/strings.rs#L192) (`lower_implode`)
- **Function symbol**: `lower_implode()`


### Lowering notes

- Lowers `implode(glue, array)` by selecting the string or integer array helper.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function implode(string $separator, array $array): string
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Cross-references

- [User reference for `implode()`](../../../php/builtins/string/implode.md)

