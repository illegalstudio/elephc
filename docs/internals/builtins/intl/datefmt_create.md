---
title: "datefmt_create() - internals"
description: "Compiler internals for datefmt_create(): lowering path, type checks, and runtime helpers."
---

## `datefmt_create()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_intl.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_intl.rs)
- **Lowering**: [`src/intl/functions.rs`](https://github.com/illegalstudio/elephc/blob/main/src/intl/functions.rs) (`datefmt_create`)
- **Function symbol**: `datefmt_create()`


### Lowering notes

- Implemented by the compiler-injected Intl prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function datefmt_create(?string $locale, int $dateType = IntlDateFormatter::FULL, int $timeType = IntlDateFormatter::FULL, object|string|null $timezone = null, object|int|null $calendar = null, ?string $pattern = null): ?object
```

## What the type checker enforces

- **Arity**: takes 1–6 arguments (5 optional).

## Eval interpreter (magician)

_Not callable from eval'd code - the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `datefmt_create()`](../../../php/builtins/intl/datefmt_create.md)
