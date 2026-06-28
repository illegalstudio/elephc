---
title: "php_uname() — internals"
description: "Compiler internals for php_uname(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 266
---

## `php_uname()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/system.rs`:672](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/system.rs#L672) (`lower_php_uname`)
- **Function symbol**: `lower_php_uname()`


### Lowering notes

- Lowers `php_uname(mode?)` through the target-aware uname runtime helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_php_uname`

## Signature summary

```php
function php_uname(string $mode): string
```

## What the type checker enforces

- **Arity**: takes 0–1 arguments (1 optional).

## Cross-references

- [User reference for `php_uname()`](../../../php/builtins/misc/php_uname.md)

