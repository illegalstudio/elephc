---
title: "elephc_trace_start() - internals"
description: "Compiler internals for elephc_trace_start(): lowering path, type checks, and runtime helpers."
---

## `elephc_trace_start()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/debug_prelude.rs`](https://github.com/illegalstudio/elephc/blob/main/src/debug_prelude.rs) (`elephc_trace_start`)
- **Function symbol**: `elephc_trace_start()`


### Lowering notes

- Implemented by the compiler-injected debug prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function elephc_trace_start(string $name): void
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

_Not callable from eval'd code - the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `elephc_trace_start()`](../../../php/builtins/debug/elephc_trace_start.md)
