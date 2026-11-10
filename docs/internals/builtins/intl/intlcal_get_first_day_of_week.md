---
title: "intlcal_get_first_day_of_week() - internals"
description: "Compiler internals for intlcal_get_first_day_of_week(): lowering path, type checks, and runtime helpers."
---

## `intlcal_get_first_day_of_week()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_intl.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_intl.rs)
- **Lowering**: [`src/intl/functions.rs`](https://github.com/illegalstudio/elephc/blob/main/src/intl/functions.rs) (`intlcal_get_first_day_of_week`)
- **Function symbol**: `intlcal_get_first_day_of_week()`


### Lowering notes

- Implemented by the compiler-injected Intl prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function intlcal_get_first_day_of_week(object $calendar): int|false
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

_Not callable from eval'd code - the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `intlcal_get_first_day_of_week()`](../../../php/builtins/intl/intlcal_get_first_day_of_week.md)
