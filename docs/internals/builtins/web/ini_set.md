---
title: "ini_set() — internals"
description: "Compiler internals for ini_set(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 849
---

## `ini_set()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/web_prelude/build.rs`:4452](https://github.com/illegalstudio/elephc/blob/main/src/web_prelude/build.rs#L4452) (`ini_set`)
- **Function symbol**: `ini_set()`


### Lowering notes

- Implemented by the compiler-injected web prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function ini_set(string $option, mixed $value): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `ini_set()`](../../../php/builtins/web/ini_set.md)
