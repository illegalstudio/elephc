---
title: "rawurldecode()"
description: "Decodes an RFC 3986 percent-encoded string without treating '+' as a space."
---

## rawurldecode()

```php
function rawurldecode(string $string): string
```

Decodes an RFC 3986 percent-encoded string without treating '+' as a space.

**Parameters**:
- `$string` (`string`)

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `rawurldecode` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/rawurldecode.md).
