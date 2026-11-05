---
title: "url_percent_encode() - internals"
description: "Compiler internals for url_percent_encode(): lowering path, type checks, and runtime helpers."
---

## `url_percent_encode()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/uri_prelude/builders.rs`](https://github.com/illegalstudio/elephc/blob/main/src/uri_prelude/builders.rs) (`url_percent_encode`)
- **Function symbol**: `url_percent_encode()`


### Lowering notes

- Implemented by the compiler-injected uri prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function url_percent_encode(string $input, mixed $mode): string
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

_Not callable from eval'd code - the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `url_percent_encode()`](../../../php/builtins/uri/url_percent_encode.md)
