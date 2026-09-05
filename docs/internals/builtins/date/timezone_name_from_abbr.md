---
title: "timezone_name_from_abbr() — internals"
description: "Compiler internals for timezone_name_from_abbr(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 246
---

## `timezone_name_from_abbr()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_surfaces.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_surfaces.rs)
- **Lowering**: [`src/name_resolver/expressions.rs`:643](https://github.com/illegalstudio/elephc/blob/main/src/name_resolver/expressions.rs#L643) (`rewrite_date_procedural_call`)
- **Function symbol**: `rewrite_date_procedural_call()`


### Lowering notes

- Rewritten by the name resolver into a constructor or method call on the corresponding builtin class before type checking.

## Semantic descriptor

Shared contract without a registry semantic descriptor.

## EIR and runtime boundary

_No registry-backed typed runtime target applies._

## Signature summary

```php
function timezone_name_from_abbr(string $abbr, int $utcOffset = -1, int $isDST = -1): mixed
```

## What the type checker enforces

- **Arity**: takes 1–3 arguments (2 optional).

## Eval interpreter (magician)

Dispatched as a procedural date/time alias by [`crates/elephc-magician/src/interpreter/builtins/time/aliases.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/time/aliases.rs).

## Cross-references

- [User reference for `timezone_name_from_abbr()`](../../../php/builtins/date/timezone_name_from_abbr.md)
