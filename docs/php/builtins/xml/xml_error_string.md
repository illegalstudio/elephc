---
title: "xml_error_string()"
description: "Returns the message for an XML parser error code."
---

## xml_error_string()

```php
function xml_error_string(int $error_code): ?string
```

Returns the message for an XML parser error code.

**Parameters**:
- `$error_code` (`int`)

**Returns**: `?string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `xml_error_string` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/xml_error_string.md).
