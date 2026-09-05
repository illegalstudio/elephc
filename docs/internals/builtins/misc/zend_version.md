---
title: "zend_version() — internals"
description: "Compiler internals for zend_version(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 628
---

## `zend_version()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/version_prelude.rs`:36](https://github.com/illegalstudio/elephc/blob/main/src/version_prelude.rs#L36) (`zend_version`)
- **Function symbol**: `zend_version()`


### Lowering notes

- Implemented by the compiler-injected version prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function zend_version(): string
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `zend_version()`](../../../php/builtins/misc/zend_version.md)
