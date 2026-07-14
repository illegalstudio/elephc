---
title: "vprintf() — internals"
description: "Compiler internals for vprintf(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 411
---

## `vprintf()` — internals

## Where it lives

- **Signature**: [`src/builtins/string/vprintf.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/string/vprintf.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/strings.rs`:548](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/strings.rs#L548) (`lower_vprintf`)
- **Function symbol**: `lower_vprintf()`


### Lowering notes

- Lowers `vprintf(format, values)` as `vsprintf()` followed by stdout emission.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_sprintf`

## Signature summary

```php
function vprintf(string $format, array $values): int
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/formatting/vprintf.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/formatting/vprintf.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `vprintf()`](../../../php/builtins/string/vprintf.md)
