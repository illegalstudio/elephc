---
title: "timezone_identifiers_list() — internals"
description: "Compiler internals for timezone_identifiers_list(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 244
---

## `timezone_identifiers_list()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_surfaces.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_surfaces.rs)
- **Lowering**: [`src/name_resolver/expressions.rs`:780](https://github.com/illegalstudio/elephc/blob/main/src/name_resolver/expressions.rs#L780) (`rewrite_date_procedural_call`)
- **Function symbol**: `rewrite_date_procedural_call()`


### Lowering notes

- Rewritten by the name resolver into a constructor or method call on the corresponding builtin class before type checking.

## Semantic descriptor

Shared contract without a registry semantic descriptor.

## EIR and runtime boundary

_No registry-backed typed runtime target applies._

## Signature summary

```php
function timezone_identifiers_list(int $timezoneGroup = DateTimeZone::ALL, string $countryCode = null): mixed
```

## What the type checker enforces

- **Arity**: takes 0–2 arguments (2 optional).

## Eval interpreter (magician)

Dispatched as a procedural date/time alias by [`crates/elephc-magician/src/interpreter/builtins/time/aliases.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/time/aliases.rs).

## Cross-references

- [User reference for `timezone_identifiers_list()`](../../../php/builtins/date/timezone_identifiers_list.md)
