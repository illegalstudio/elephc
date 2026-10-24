---
title: "simplexml_import_dom() - internals"
description: "Compiler internals for simplexml_import_dom(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 1011
---

## `simplexml_import_dom()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_xml.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_xml.rs)
- **Lowering**: [`src/simplexml_prelude/surface.rs`:362](https://github.com/illegalstudio/elephc/blob/main/src/simplexml_prelude/surface.rs#L362) (`simplexml_import_dom`)
- **Function symbol**: `simplexml_import_dom()`


### Lowering notes

- Implemented by the compiler-injected xml prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function simplexml_import_dom(object $node, ?string $class_name = 'SimpleXMLElement'): ?SimpleXMLElement
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/xml/simplexml_import_dom.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/simplexml_import_dom.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `simplexml_import_dom()`](../../../php/builtins/xml/simplexml_import_dom.md)
