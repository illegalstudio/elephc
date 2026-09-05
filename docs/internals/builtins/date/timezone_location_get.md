---
title: "timezone_location_get() — internals"
description: "Compiler internals for timezone_location_get(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 245
---

## `timezone_location_get()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/tz_prelude.rs`:59](https://github.com/illegalstudio/elephc/blob/main/src/tz_prelude.rs#L59) (`timezone_location_get`)
- **Function symbol**: `timezone_location_get()`


### Lowering notes

- Implemented by the compiler-injected tz prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function timezone_location_get(mixed $object): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

Dispatched as a procedural date/time alias by [`crates/elephc-magician/src/interpreter/builtins/time/aliases.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/time/aliases.rs).

## Cross-references

- [User reference for `timezone_location_get()`](../../../php/builtins/date/timezone_location_get.md)
