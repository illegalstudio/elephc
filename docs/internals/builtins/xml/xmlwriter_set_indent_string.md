---
title: "xmlwriter_set_indent_string() - internals"
description: "Compiler internals for xmlwriter_set_indent_string(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 974
---

## `xmlwriter_set_indent_string()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_xml.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_xml.rs)
- **Lowering**: [`src/xml_prelude/build/writer.rs`:995](https://github.com/illegalstudio/elephc/blob/main/src/xml_prelude/build/writer.rs#L995) (`xmlwriter_set_indent_string`)
- **Function symbol**: `xmlwriter_set_indent_string()`


### Lowering notes

- Implemented by the compiler-injected xml prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function xmlwriter_set_indent_string(mixed $writer, string $indentation): bool
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_set_indent_string.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_set_indent_string.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `xmlwriter_set_indent_string()`](../../../php/builtins/xml/xmlwriter_set_indent_string.md)
