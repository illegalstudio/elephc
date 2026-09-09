---
title: "xmlwriter_start_pi() - internals"
description: "Compiler internals for xmlwriter_start_pi(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 986
---

## `xmlwriter_start_pi()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_xml.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_xml.rs)
- **Lowering**: [`src/xml_prelude/build/writer.rs`:1170](https://github.com/illegalstudio/elephc/blob/main/src/xml_prelude/build/writer.rs#L1170) (`xmlwriter_start_pi`)
- **Function symbol**: `xmlwriter_start_pi()`


### Lowering notes

- Implemented by the compiler-injected xml prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function xmlwriter_start_pi(mixed $writer, string $target): bool
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_start_pi.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_start_pi.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `xmlwriter_start_pi()`](../../../php/builtins/xml/xmlwriter_start_pi.md)
