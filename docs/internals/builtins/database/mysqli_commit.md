---
title: "mysqli_commit() — internals"
description: "Compiler internals for mysqli_commit(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 103
---

## `mysqli_commit()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/mysqli_prelude/build/procedural.rs`:291](https://github.com/illegalstudio/elephc/blob/main/src/mysqli_prelude/build/procedural.rs#L291) (`mysqli_commit`)
- **Function symbol**: `mysqli_commit()`


### Lowering notes

- Implemented by the compiler-injected mysqli prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function mysqli_commit(mixed $mysql, int $flags = 0, ?string $name = null): bool
```

## What the type checker enforces

- **Arity**: takes 1–3 arguments (2 optional).

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `mysqli_commit()`](../../../php/builtins/database/mysqli_commit.md)
