---
title: "chr() — internals"
description: "Compiler internals for chr(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 361
---

## `chr()` — internals

## Where it lives

- **Signature**: [`src/builtins/string/chr.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/string/chr.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/strings.rs`:920](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/strings.rs#L920) (`lower_chr`)
- **Function symbol**: `lower_chr()`


### Lowering notes

- Lowers `chr()` by converting an integer code point into a one-byte string.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_chr`

## Signature summary

```php
function chr(int $codepoint): string
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/string/chr.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/chr.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `chr()`](../../../php/builtins/string/chr.md)
