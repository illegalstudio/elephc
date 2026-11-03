---
title: "iconv()"
description: "Converts a string from one character encoding to another."
---

## iconv()

```php
function iconv(string $from_encoding, string $to_encoding, string $string): mixed
```

Converts a string from one character encoding to another.

**Parameters**:
- `$from_encoding` (`string`)
- `$to_encoding` (`string`)
- `$string` (`string`)

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `iconv` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/iconv.md).
