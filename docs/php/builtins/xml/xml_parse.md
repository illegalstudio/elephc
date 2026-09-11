---
title: "xml_parse()"
description: "Parses a chunk of XML data, dispatching the registered handlers."
sidebar:
  order: 917
---

## xml_parse()

```php
function xml_parse(mixed $parser, string $data, bool $is_final = false): int
```

Parses a chunk of XML data, dispatching the registered handlers.

**Parameters**:
- `$parser` (`mixed`)
- `$data` (`string`)
- `$is_final` (`bool`), default `false`, optional

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/xml/xml_parse.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xml_parse.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `xml_parse` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/xml_parse.md).
