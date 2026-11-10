---
title: "intlcal_get_repeated_wall_time_option() - internals"
description: "Compiler internals for intlcal_get_repeated_wall_time_option(): lowering path, type checks, and runtime helpers."
---

## `intlcal_get_repeated_wall_time_option()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_intl.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_intl.rs)
- **Lowering**: [`src/intl/functions.rs`](https://github.com/illegalstudio/elephc/blob/main/src/intl/functions.rs) (`intlcal_get_repeated_wall_time_option`)
- **Function symbol**: `intlcal_get_repeated_wall_time_option()`


### Lowering notes

- Implemented by the compiler-injected Intl prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function intlcal_get_repeated_wall_time_option(object $calendar): int
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

_Not callable from eval'd code - the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `intlcal_get_repeated_wall_time_option()`](../../../php/builtins/intl/intlcal_get_repeated_wall_time_option.md)
