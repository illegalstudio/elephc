---
title: "intlcal_set() - internals"
description: "Compiler internals for intlcal_set(): lowering path, type checks, and runtime helpers."
---

## `intlcal_set()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_intl.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_intl.rs)
- **Lowering**: [`src/intl/functions.rs`](https://github.com/illegalstudio/elephc/blob/main/src/intl/functions.rs) (`intlcal_set`)
- **Function symbol**: `intlcal_set()`


### Lowering notes

- Implemented by the compiler-injected Intl prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function intlcal_set(object $calendar, int $year, int $month, int $dayOfMonth = null, int $hour = null, int $minute = null, int $second = null): bool
```

## What the type checker enforces

- **Arity**: takes 3–7 arguments (4 optional).

## Eval interpreter (magician)

_Not callable from eval'd code - the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `intlcal_set()`](../../../php/builtins/intl/intlcal_set.md)
