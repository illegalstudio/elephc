---
title: "xml_parse()"
description: "Parses a chunk of XML data, dispatching the registered handlers."
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

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `xml_parse` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/xml_parse.md).
