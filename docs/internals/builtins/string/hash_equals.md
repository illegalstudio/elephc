---
title: "hash_equals() — internals"
description: "Compiler internals for hash_equals(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 338
---

## `hash_equals()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/strings.rs`:247](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/strings.rs#L247) (`lower_hash_equals`)
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

## Cross-references

- [User reference for `hash_equals()`](../../../php/builtins/string/hash_equals.md)

