---
title: "vsprintf() — internals"
description: "Compiler internals for vsprintf(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 426
---

## `vsprintf()` — internals

## Where it lives

- **Signature**: [`src/builtins/string/vsprintf.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/string/vsprintf.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/strings.rs`:587](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/strings.rs#L587) (`lower_vsprintf`)
- **Function symbol**: `lower_vsprintf()`


### Lowering notes

- Lowers `vsprintf(format, values)` through the array-to-sprintf runtime bridge.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_sprintf`

## Signature summary

```php
function vsprintf(string $format, array $values): string
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/formatting/vsprintf.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/formatting/vsprintf.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `vsprintf()`](../../../php/builtins/string/vsprintf.md)
