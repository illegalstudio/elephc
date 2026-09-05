---
title: "cal_to_jd() — internals"
description: "Compiler internals for cal_to_jd(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 186
---

## `cal_to_jd()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_surfaces.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_surfaces.rs)
- **Lowering**: [`src/name_resolver/expressions.rs`:647](https://github.com/illegalstudio/elephc/blob/main/src/name_resolver/expressions.rs#L647) (`rewrite_date_procedural_call`)
- **Function symbol**: `rewrite_date_procedural_call()`


### Lowering notes

- Rewritten by the name resolver into a constructor or method call on the corresponding builtin class before type checking.

## Semantic descriptor

Shared contract without a registry semantic descriptor.

## EIR and runtime boundary

_No registry-backed typed runtime target applies._

## Signature summary

```php
function cal_to_jd(int $calendar, int $month, int $day, int $year): int
```

## What the type checker enforces

- **Arity**: takes exactly 4 arguments.

## Eval interpreter (magician)

Dispatched as a procedural date/time alias by [`crates/elephc-magician/src/interpreter/builtins/time/aliases.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/time/aliases.rs).

## Cross-references

- [User reference for `cal_to_jd()`](../../../php/builtins/date/cal_to_jd.md)
