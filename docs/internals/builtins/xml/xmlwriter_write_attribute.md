---
title: "xmlwriter_write_attribute() - internals"
description: "Compiler internals for xmlwriter_write_attribute(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 988
---

## `xmlwriter_write_attribute()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_xml.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_xml.rs)
- **Lowering**: [`src/xml_prelude/build/writer.rs`:1052](https://github.com/illegalstudio/elephc/blob/main/src/xml_prelude/build/writer.rs#L1052) (`xmlwriter_write_attribute`)
- **Function symbol**: `xmlwriter_write_attribute()`


### Lowering notes

- Implemented by the compiler-injected xml prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function xmlwriter_write_attribute(mixed $writer, string $name, string $value): bool
```

## What the type checker enforces

- **Arity**: takes exactly 3 arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_write_attribute.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_write_attribute.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `xmlwriter_write_attribute()`](../../../php/builtins/xml/xmlwriter_write_attribute.md)
