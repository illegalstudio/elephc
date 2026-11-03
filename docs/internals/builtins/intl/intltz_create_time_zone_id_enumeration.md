---
title: "intltz_create_time_zone_id_enumeration() - internals"
description: "Compiler internals for intltz_create_time_zone_id_enumeration(): lowering path, type checks, and runtime helpers."
---

## `intltz_create_time_zone_id_enumeration()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_intl.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_intl.rs)
- **Lowering**: [`src/intl/functions.rs`](https://github.com/illegalstudio/elephc/blob/main/src/intl/functions.rs) (`intltz_create_time_zone_id_enumeration`)
- **Function symbol**: `intltz_create_time_zone_id_enumeration()`


### Lowering notes

- Implemented by the compiler-injected Intl prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function intltz_create_time_zone_id_enumeration(int $type, ?string $region = null, ?int $rawOffset = null): object|false
```

## What the type checker enforces

- **Arity**: takes 1–3 arguments (2 optional).

## Eval interpreter (magician)

_Not callable from eval'd code - the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `intltz_create_time_zone_id_enumeration()`](../../../php/builtins/intl/intltz_create_time_zone_id_enumeration.md)
