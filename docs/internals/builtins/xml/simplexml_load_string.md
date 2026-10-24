---
title: "simplexml_load_string() - internals"
description: "Compiler internals for simplexml_load_string(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 1013
---

## `simplexml_load_string()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_xml.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_xml.rs)
- **Lowering**: [`src/simplexml_prelude/surface.rs`:275](https://github.com/illegalstudio/elephc/blob/main/src/simplexml_prelude/surface.rs#L275) (`simplexml_load_string`)
- **Function symbol**: `simplexml_load_string()`


### Lowering notes

- Implemented by the compiler-injected xml prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function simplexml_load_string(string $data, ?string $class_name = 'SimpleXMLElement', int $options = 0, string $namespace_or_prefix = '', bool $is_prefix = false): mixed
```

## What the type checker enforces

- **Arity**: takes 1–5 arguments (4 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/xml/simplexml_load_string.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/simplexml_load_string.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `simplexml_load_string()`](../../../php/builtins/xml/simplexml_load_string.md)
