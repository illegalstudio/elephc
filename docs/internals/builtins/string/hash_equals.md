---
title: "hash_equals() — internals"
description: "Compiler internals for hash_equals(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 359
---

## `hash_equals()` — internals

## Where it lives

- **Signature**: [`src/builtins/string/hash_equals.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/string/hash_equals.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/strings.rs`:265](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/strings.rs#L265) (`lower_hash_equals`)
- **Function symbol**: `lower_hash_equals()`


### Lowering notes

- Lowers `hash_equals(known, user)` through the timing-safe runtime compare helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_hash_algos_list`
- `__rt_hash_equals`
- `__rt_hash_init`

## Signature summary

```php
function hash_equals(string $known_string, string $user_string): bool
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/string/hash_equals.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/hash_equals.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `hash_equals()`](../../../php/builtins/string/hash_equals.md)
