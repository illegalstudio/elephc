---
title: "php_uname() — internals"
description: "Compiler internals for php_uname(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 282
---

## `php_uname()` — internals

## Where it lives

- **Signature**: [`src/builtins/system/php_uname.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/system/php_uname.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/system.rs`:726](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/system.rs#L726) (`lower_php_uname`)
- **Function symbol**: `lower_php_uname()`


### Lowering notes

- Lowers `php_uname(mode?)` through the target-aware uname runtime helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_php_uname`

## Signature summary

```php
function php_uname(string $mode = 'a'): string
```

## What the type checker enforces

- **Arity**: takes 0–1 arguments (1 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/network_env/php_uname.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/network_env/php_uname.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `php_uname()`](../../../php/builtins/misc/php_uname.md)
