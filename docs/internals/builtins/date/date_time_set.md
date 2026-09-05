---
title: "date_time_set() — internals"
description: "Compiler internals for date_time_set(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 211
---

## `date_time_set()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_surfaces.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_surfaces.rs)
- **Lowering**: [`src/name_resolver/expressions.rs`:811](https://github.com/illegalstudio/elephc/blob/main/src/name_resolver/expressions.rs#L811) (`rewrite_date_procedural_call`)
- **Function symbol**: `rewrite_date_procedural_call()`


### Lowering notes

- Rewritten by the name resolver into a constructor or method call on the corresponding builtin class before type checking.

## Semantic descriptor

Shared contract without a registry semantic descriptor.

## EIR and runtime boundary

_No registry-backed typed runtime target applies._

## Signature summary

```php
function date_time_set(mixed $object, int $hour, int $minute, int $second = 0, int $microsecond = 0): mixed
```

## What the type checker enforces

- **Arity**: takes 3–5 arguments (2 optional).

## Eval interpreter (magician)

Dispatched as a procedural date/time alias by [`crates/elephc-magician/src/interpreter/builtins/time/aliases.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/time/aliases.rs).

## Cross-references

- [User reference for `date_time_set()`](../../../php/builtins/date/date_time_set.md)
