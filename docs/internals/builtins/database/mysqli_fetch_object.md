---
title: "mysqli_fetch_object() — internals"
description: "Compiler internals for mysqli_fetch_object(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 122
---

## `mysqli_fetch_object()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/mysqli_prelude/build/procedural.rs`:990](https://github.com/illegalstudio/elephc/blob/main/src/mysqli_prelude/build/procedural.rs#L990) (`mysqli_fetch_object`)
- **Function symbol**: `mysqli_fetch_object()`


### Lowering notes

- Implemented by the compiler-injected mysqli prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function mysqli_fetch_object(mixed $result, string $class = 'stdClass', mixed $constructor_args = []): mixed
```

## What the type checker enforces

- **Arity**: takes 1–3 arguments (2 optional).

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `mysqli_fetch_object()`](../../../php/builtins/database/mysqli_fetch_object.md)
