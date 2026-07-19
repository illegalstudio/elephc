---
title: "array_replace() — internals"
description: "Compiler internals for array_replace(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 32
---

## `array_replace()` — internals

## Where it lives

- **Signature**: [`src/builtins/array/array_replace.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/array/array_replace.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/arrays.rs`:1340](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/arrays.rs#L1340) (`lower_array_replace`)
- **Function symbol**: `lower_array_replace()`


### Lowering notes

- Lowers `array_replace()` (right-wins hash merge of two hashes).

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_array_replace`
- `__rt_array_replace_recursive`
- `__rt_assoc_diff_intersect`

## Signature summary

```php
function array_replace(array $array, array $replacements): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `array_replace()`](../../../php/builtins/array/array_replace.md)
