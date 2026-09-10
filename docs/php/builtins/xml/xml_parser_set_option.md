---
title: "xml_parser_set_option()"
description: "Sets an XML_OPTION_* parser option."
sidebar:
  order: 983
---

## xml_parser_set_option()

```php
function xml_parser_set_option(mixed $parser, int $option, mixed $value): bool
```

Sets an XML_OPTION_* parser option.

**Parameters**:
- `$parser` (`mixed`)
- `$option` (`int`)
- `$value` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: supported - declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/xml/xml_parser_set_option.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xml_parser_set_option.rs)).

_No examples yet - check `examples/` and `showcases/` for usage patterns._

## Internals

For how `xml_parser_set_option` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/xml_parser_set_option.md).
