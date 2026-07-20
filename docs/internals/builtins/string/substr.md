---
title: "substr() — internals"
description: "Compiler internals for substr(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 418
---

## `substr()` — internals

## Where it lives

- **Signature**: [`src/builtins/string/substr.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/string/substr.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/strings.rs`:775](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/strings.rs#L775) (`lower_substr`)
- **Function symbol**: `lower_substr()`


### Lowering notes

- Lowers `substr(string, offset, length?)` with target-local pointer arithmetic.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_substr_replace`

## Signature summary

```php
function substr(string $string, int $offset, int $length = null): string
```

## What the type checker enforces

- **Arity**: takes 2–3 arguments (1 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/string/substr.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/substr.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `substr()`](../../../php/builtins/string/substr.md)
