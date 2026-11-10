---
title: "intlcal_from_date_time() - internals"
description: "Compiler internals for intlcal_from_date_time(): lowering path, type checks, and runtime helpers."
---

## `intlcal_from_date_time()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_intl.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_intl.rs)
- **Lowering**: [`src/intl/functions.rs`](https://github.com/illegalstudio/elephc/blob/main/src/intl/functions.rs) (`intlcal_from_date_time`)
- **Function symbol**: `intlcal_from_date_time()`


### Lowering notes

- Implemented by the compiler-injected Intl prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function intlcal_from_date_time(object|string $datetime, ?string $locale = null): ?object
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Eval interpreter (magician)

_Not callable from eval'd code - the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `intlcal_from_date_time()`](../../../php/builtins/intl/intlcal_from_date_time.md)
