---
title: "xmlwriter_start_dtd() - internals"
description: "Compiler internals for xmlwriter_start_dtd(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 980
---

## `xmlwriter_start_dtd()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_xml.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_xml.rs)
- **Lowering**: [`src/xml_prelude/build/writer.rs`:1301](https://github.com/illegalstudio/elephc/blob/main/src/xml_prelude/build/writer.rs#L1301) (`xmlwriter_start_dtd`)
- **Function symbol**: `xmlwriter_start_dtd()`


### Lowering notes

- Implemented by the compiler-injected xml prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function xmlwriter_start_dtd(mixed $writer, string $qualifiedName, ?string $publicId = null, ?string $systemId = null): bool
```

## What the type checker enforces

- **Arity**: takes 2–4 arguments (2 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_start_dtd.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_start_dtd.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `xmlwriter_start_dtd()`](../../../php/builtins/xml/xmlwriter_start_dtd.md)
