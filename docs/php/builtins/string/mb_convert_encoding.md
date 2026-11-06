---
title: "mb_convert_encoding()"
description: "Converts strings and recursive array keys and values between character encodings."
---

## mb_convert_encoding()

```php
function mb_convert_encoding(array|string $string, string $to_encoding, array|string|null $from_encoding = null): array|string|false
```

Converts strings and recursive array keys and values between character encodings.

**Parameters**:
- `$string` (`array|string`)
- `$to_encoding` (`string`)
- `$from_encoding` (`array|string|null`), default `null`, optional

**Returns**: `array|string|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_convert_encoding` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_convert_encoding.md).
