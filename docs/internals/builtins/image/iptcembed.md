---
title: "iptcembed() — internals"
description: "Compiler internals for iptcembed(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 537
---

## `iptcembed()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/image_prelude.rs`:4063](https://github.com/illegalstudio/elephc/blob/main/src/image_prelude.rs#L4063) (`iptcembed`)
- **Function symbol**: `iptcembed()`


### Lowering notes

- Implemented by the compiler-injected image prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function iptcembed(string $iptcdata, string $jpeg_file_name, int $spool = 0): mixed
```

## What the type checker enforces

- **Arity**: takes 2–3 arguments (1 optional).

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `iptcembed()`](../../../php/builtins/image/iptcembed.md)
