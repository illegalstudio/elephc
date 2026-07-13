---
title: "explode() — internals"
description: "Compiler internals for explode(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 350
---

## `explode()` — internals

## Where it lives

- **Signature**: [`src/builtins/string/explode.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/string/explode.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/strings.rs`:169](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/strings.rs#L169) (`lower_explode`)
- **Function symbol**: `lower_explode()`


### Lowering notes

- Lowers `explode(delimiter, string)` into the shared string-array splitter helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_explode`
- `__rt_sscanf`

## Signature summary

```php
function explode(string $separator, string $string, int $limit = PHP_INT_MAX): array
```

## What the type checker enforces

- **Arity**: takes 2–3 arguments (1 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/string/explode.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/explode.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `explode()`](../../../php/builtins/string/explode.md)
