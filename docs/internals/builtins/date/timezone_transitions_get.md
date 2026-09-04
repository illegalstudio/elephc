---
title: "timezone_transitions_get() — internals"
description: "Compiler internals for timezone_transitions_get(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 250
---

## `timezone_transitions_get()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/tz_prelude.rs`:63](https://github.com/illegalstudio/elephc/blob/main/src/tz_prelude.rs#L63) (`timezone_transitions_get`)
- **Function symbol**: `timezone_transitions_get()`


### Lowering notes

- Implemented by the compiler-injected tz prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function timezone_transitions_get(mixed $object, int $timestampBegin = PHP_INT_MIN, int $timestampEnd = PHP_INT_MAX): mixed
```

## What the type checker enforces

- **Arity**: takes 1–3 arguments (2 optional).

## Eval interpreter (magician)

Dispatched as a procedural date/time alias by [`crates/elephc-magician/src/interpreter/builtins/time/aliases.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/time/aliases.rs).

## Cross-references

- [User reference for `timezone_transitions_get()`](../../../php/builtins/date/timezone_transitions_get.md)
