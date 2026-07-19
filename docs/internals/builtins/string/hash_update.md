---
title: "hash_update() — internals"
description: "Compiler internals for hash_update(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 363
---

## `hash_update()` — internals

## Where it lives

- **Signature**: [`src/builtins/string/hash_update.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/string/hash_update.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/strings.rs`:295](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/strings.rs#L295) (`lower_hash_update`)
- **Function symbol**: `lower_hash_update()`


### Lowering notes

- Lowers `hash_update(context, data)` through the incremental hash runtime helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_hash_update`

## Signature summary

```php
function hash_update(resource $context, string $data): bool
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/string/hash_update.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/hash_update.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `hash_update()`](../../../php/builtins/string/hash_update.md)
