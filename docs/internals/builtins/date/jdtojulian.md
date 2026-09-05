---
title: "jdtojulian() — internals"
description: "Compiler internals for jdtojulian(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 232
---

## `jdtojulian()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_surfaces.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_surfaces.rs)
- **Lowering**: [`src/name_resolver/expressions.rs`:660](https://github.com/illegalstudio/elephc/blob/main/src/name_resolver/expressions.rs#L660) (`rewrite_date_procedural_call`)
- **Function symbol**: `rewrite_date_procedural_call()`


### Lowering notes

- Rewritten by the name resolver into a constructor or method call on the corresponding builtin class before type checking.

## Semantic descriptor

Shared contract without a registry semantic descriptor.

## EIR and runtime boundary

_No registry-backed typed runtime target applies._

## Signature summary

```php
function jdtojulian(int $julian_day): string
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

Dispatched as a procedural date/time alias by [`crates/elephc-magician/src/interpreter/builtins/time/aliases.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/time/aliases.rs).

## Cross-references

- [User reference for `jdtojulian()`](../../../php/builtins/date/jdtojulian.md)
