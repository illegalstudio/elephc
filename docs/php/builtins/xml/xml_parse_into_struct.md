---
title: "xml_parse_into_struct()"
description: "Parses a whole XML document into an array of tag structures and an index by tag name."
---

## xml_parse_into_struct()

```php
function xml_parse_into_struct(mixed $parser, string $data, mixed $values, mixed $index = null): int
```

Parses a whole XML document into an array of tag structures and an index by tag name.

**Parameters**:
- `$parser` (`mixed`)
- `$data` (`string`)
- `$values` (`mixed`), passed by reference
- `$index` (`mixed`), passed by reference, default `null`, optional

**Returns**: `int`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `xml_parse_into_struct` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/xml_parse_into_struct.md).
