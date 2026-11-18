---
title: "libxml_get_last_error() - internals"
description: "Compiler internals for libxml_get_last_error(): lowering path, type checks, and runtime helpers."
---

## `libxml_get_last_error()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_xml.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_xml.rs)
- **Lowering**: [`src/dom_html_prelude/libxml.rs`](https://github.com/illegalstudio/elephc/blob/main/src/dom_html_prelude/libxml.rs) (`libxml_get_last_error`)
- **Function symbol**: `libxml_get_last_error()`


### Lowering notes

- Implemented by the compiler-injected libxml prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function libxml_get_last_error(): object|false
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/xml/libxml_get_last_error.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/libxml_get_last_error.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `libxml_get_last_error()`](../../../php/builtins/xml/libxml_get_last_error.md)
