---
title: "intlgregcal_is_leap_year() - internals"
description: "Compiler internals for intlgregcal_is_leap_year(): lowering path, type checks, and runtime helpers."
---

## `intlgregcal_is_leap_year()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_intl.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_intl.rs)
- **Lowering**: [`src/intl/functions.rs`](https://github.com/illegalstudio/elephc/blob/main/src/intl/functions.rs) (`intlgregcal_is_leap_year`)
- **Function symbol**: `intlgregcal_is_leap_year()`


### Lowering notes

- Implemented by the compiler-injected Intl prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function intlgregcal_is_leap_year(object $calendar, int $year): bool
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

_Not callable from eval'd code - the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `intlgregcal_is_leap_year()`](../../../php/builtins/intl/intlgregcal_is_leap_year.md)
