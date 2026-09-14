---
title: "xml_parser_get_option()"
description: "Reads an XML_OPTION_* parser option."
sidebar:
  order: 982
---

## xml_parser_get_option()

```php
function xml_parser_get_option(mixed $parser, int $option): mixed
```

Reads an XML_OPTION_* parser option.

**Parameters**:
- `$parser` (`mixed`)
- `$option` (`int`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: supported - declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/xml/xml_parser_get_option.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xml_parser_get_option.rs)).

_No examples yet - check `examples/` and `showcases/` for usage patterns._

## Internals

For how `xml_parser_get_option` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/xml_parser_get_option.md).
