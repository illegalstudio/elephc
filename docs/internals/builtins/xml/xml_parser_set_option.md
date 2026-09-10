---
title: "xml_parser_set_option() - internals"
description: "Compiler internals for xml_parser_set_option(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 947
---

## `xml_parser_set_option()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_xml.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_xml.rs)
- **Lowering**: [`src/xml_prelude/build/parser.rs`:1576](https://github.com/illegalstudio/elephc/blob/main/src/xml_prelude/build/parser.rs#L1576) (`xml_parser_set_option`)
- **Function symbol**: `xml_parser_set_option()`


### Lowering notes

- Implemented by the compiler-injected xml prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function xml_parser_set_option(mixed $parser, int $option, mixed $value): bool
```

## What the type checker enforces

- **Arity**: takes exactly 3 arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/xml/xml_parser_set_option.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xml_parser_set_option.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `xml_parser_set_option()`](../../../php/builtins/xml/xml_parser_set_option.md)
