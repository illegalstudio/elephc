---
title: "rawurlencode()"
description: "URL-encodes a string using RFC 3986 percent-encoding (no '+' for spaces)."
---

## rawurlencode()

```php
function rawurlencode(string $string): string
```

URL-encodes a string using RFC 3986 percent-encoding (no '+' for spaces).

**Parameters**:
- `$string` (`string`)

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `rawurlencode` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/rawurlencode.md).
