---
title: "mysqli_get_links_stats() - internals"
description: "Compiler internals for mysqli_get_links_stats(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 141
---

## `mysqli_get_links_stats()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/mysqli_prelude/build/procedural.rs`:718](https://github.com/illegalstudio/elephc/blob/main/src/mysqli_prelude/build/procedural.rs#L718) (`mysqli_get_links_stats`)
- **Function symbol**: `mysqli_get_links_stats()`


### Lowering notes

- Implemented by the compiler-injected mysqli prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function mysqli_get_links_stats(): array
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Eval interpreter (magician)

_Not callable from eval'd code - the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `mysqli_get_links_stats()`](../../../php/builtins/database/mysqli_get_links_stats.md)
