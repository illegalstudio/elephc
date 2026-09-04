---
title: "ini_restore() — internals"
description: "Compiler internals for ini_restore(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 610
---

## `ini_restore()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/version_prelude.rs`:74](https://github.com/illegalstudio/elephc/blob/main/src/version_prelude.rs#L74) (`ini_restore`)
- **Function symbol**: `ini_restore()`


### Lowering notes

- Implemented by the compiler-injected version prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function ini_restore(string $option): void
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `ini_restore()`](../../../php/builtins/misc/ini_restore.md)
