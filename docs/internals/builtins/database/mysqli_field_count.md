---
title: "mysqli_field_count() — internals"
description: "Compiler internals for mysqli_field_count(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 124
---

## `mysqli_field_count()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/mysqli_prelude/build/procedural.rs`:729](https://github.com/illegalstudio/elephc/blob/main/src/mysqli_prelude/build/procedural.rs#L729) (`mysqli_field_count`)
- **Function symbol**: `mysqli_field_count()`


### Lowering notes

- Implemented by the compiler-injected mysqli prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function mysqli_field_count(mixed $mysql): int
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `mysqli_field_count()`](../../../php/builtins/database/mysqli_field_count.md)
