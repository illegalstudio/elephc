---
title: "intlgregcal_create_instance() - internals"
description: "Compiler internals for intlgregcal_create_instance(): lowering path, type checks, and runtime helpers."
---

## `intlgregcal_create_instance()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_intl.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_intl.rs)
- **Lowering**: [`src/intl/functions.rs`](https://github.com/illegalstudio/elephc/blob/main/src/intl/functions.rs) (`intlgregcal_create_instance`)
- **Function symbol**: `intlgregcal_create_instance()`


### Lowering notes

- Implemented by the compiler-injected Intl prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function intlgregcal_create_instance(mixed $timezoneOrYear = null, mixed $localeOrMonth = null, mixed $day = null, mixed $hour = null, mixed $minute = null, mixed $second = null): ?object
```

## What the type checker enforces

- **Arity**: takes 0–6 arguments (6 optional).

## Eval interpreter (magician)

_Not callable from eval'd code - the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `intlgregcal_create_instance()`](../../../php/builtins/intl/intlgregcal_create_instance.md)
