---
title: "libxml_clear_errors() - internals"
description: "Compiler internals for libxml_clear_errors(): lowering path, type checks, and runtime helpers."
---

## `libxml_clear_errors()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_xml.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_xml.rs)
- **Lowering**: [`src/xml_prelude/build/libxml.rs`](https://github.com/illegalstudio/elephc/blob/main/src/xml_prelude/build/libxml.rs) (`libxml_clear_errors`)
- **Function symbol**: `libxml_clear_errors()`


### Lowering notes

- Implemented by the compiler-injected xml prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function libxml_clear_errors(): void
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/xml/libxml_clear_errors.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/libxml_clear_errors.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `libxml_clear_errors()`](../../../php/builtins/xml/libxml_clear_errors.md)
