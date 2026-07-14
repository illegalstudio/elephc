---
title: "ord() — internals"
description: "Compiler internals for ord(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 379
---

## `ord()` — internals

## Where it lives

- **Signature**: [`src/builtins/string/ord.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/string/ord.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/strings.rs`:897](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/strings.rs#L897) (`lower_ord`)
- **Function symbol**: `lower_ord()`


### Lowering notes

- Lowers `ord()` by returning the first byte of a string or zero for empty input.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function ord(string $character): int
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/string/ord.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/ord.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `ord()`](../../../php/builtins/string/ord.md)
