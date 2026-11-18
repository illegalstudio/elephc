---
title: "libxml_disable_entity_loader() - internals"
description: "Compiler internals for libxml_disable_entity_loader(): lowering path, type checks, and runtime helpers."
---

## `libxml_disable_entity_loader()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_xml.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_xml.rs)
- **Lowering**: [`src/dom_html_prelude/libxml.rs`](https://github.com/illegalstudio/elephc/blob/main/src/dom_html_prelude/libxml.rs) (`libxml_disable_entity_loader`)
- **Function symbol**: `libxml_disable_entity_loader()`


### Lowering notes

- Implemented by the compiler-injected libxml prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function libxml_disable_entity_loader(bool $disable = true): bool
```

## What the type checker enforces

- **Arity**: takes 0–1 arguments (1 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/xml/libxml_disable_entity_loader.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/libxml_disable_entity_loader.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `libxml_disable_entity_loader()`](../../../php/builtins/xml/libxml_disable_entity_loader.md)
