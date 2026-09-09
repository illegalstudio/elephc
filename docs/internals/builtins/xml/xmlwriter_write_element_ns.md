---
title: "xmlwriter_write_element_ns() — internals"
description: "Compiler internals for xmlwriter_write_element_ns(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 972
---

## `xmlwriter_write_element_ns()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_xml.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_xml.rs)
- **Lowering**: [`src/xml_prelude/build/writer.rs`:1136](https://github.com/illegalstudio/elephc/blob/main/src/xml_prelude/build/writer.rs#L1136) (`xmlwriter_write_element_ns`)
- **Function symbol**: `xmlwriter_write_element_ns()`


### Lowering notes

- Implemented by the compiler-injected xml prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function xmlwriter_write_element_ns(mixed $writer, ?string $prefix, string $name, ?string $namespace, ?string $content = null): bool
```

## What the type checker enforces

- **Arity**: takes 4–5 arguments (1 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_write_element_ns.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_write_element_ns.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `xmlwriter_write_element_ns()`](../../../php/builtins/xml/xmlwriter_write_element_ns.md)
