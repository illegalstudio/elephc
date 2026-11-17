---
title: "xml_parser_create_ns()"
description: "Creates a namespace-aware XML parser whose qualified names join the URI and local name with a separator."
---

## xml_parser_create_ns()

```php
function xml_parser_create_ns(?string $encoding = null, string $separator = ':'): mixed
```

Creates a namespace-aware XML parser whose qualified names join the URI and local name with a separator.

**Parameters**:
- `$encoding` (`?string`), default `null`, optional
- `$separator` (`string`), default `':'`, optional

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `xml_parser_create_ns` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/xml_parser_create_ns.md).
