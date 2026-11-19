---
title: "pack() - internals"
description: "Compiler internals for pack(): lowering path, type checks, and runtime helpers."
---

## `pack()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/pack_prelude.rs`](https://github.com/illegalstudio/elephc/blob/main/src/pack_prelude.rs) (`pack`)
- **Function symbol**: `pack()`


### Lowering notes

- Implemented by the compiler-injected pack prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function pack(string $format, ...$values): string
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.
- **Variadic**: collects excess arguments into `$values`.

## Eval interpreter (magician)

_Not callable from eval'd code - the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `pack()`](../../../php/builtins/string/pack.md)
