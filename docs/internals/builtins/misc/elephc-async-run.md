---
title: "elephc\async\run() - internals"
description: "Compiler internals for elephc\async\run(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 609
---

## `elephc\async\run()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_surfaces.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_surfaces.rs)
- **Lowering**: [`src/async_prelude/source.php`:1215](https://github.com/illegalstudio/elephc/blob/main/src/async_prelude/source.php#L1215) (`elephc\async\run`)
- **Function symbol**: `elephc\async\run()`


### Lowering notes

- Implemented by the compiler-injected Async scheduler prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function elephc\async\run(callable $body): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

_Not callable from eval'd code - the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `elephc\async\run()`](../../../php/builtins/misc/elephc-async-run.md)
