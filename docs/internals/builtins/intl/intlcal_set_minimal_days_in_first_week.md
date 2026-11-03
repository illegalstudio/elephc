---
title: "intlcal_set_minimal_days_in_first_week() - internals"
description: "Compiler internals for intlcal_set_minimal_days_in_first_week(): lowering path, type checks, and runtime helpers."
---

## `intlcal_set_minimal_days_in_first_week()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_intl.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_intl.rs)
- **Lowering**: [`src/intl/functions.rs`](https://github.com/illegalstudio/elephc/blob/main/src/intl/functions.rs) (`intlcal_set_minimal_days_in_first_week`)
- **Function symbol**: `intlcal_set_minimal_days_in_first_week()`


### Lowering notes

- Implemented by the compiler-injected Intl prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function intlcal_set_minimal_days_in_first_week(object $calendar, int $days): bool
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

_Not callable from eval'd code - the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `intlcal_set_minimal_days_in_first_week()`](../../../php/builtins/intl/intlcal_set_minimal_days_in_first_week.md)
