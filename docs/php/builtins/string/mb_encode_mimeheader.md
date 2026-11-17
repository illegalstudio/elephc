---
title: "mb_encode_mimeheader()"
description: "Encodes a MIME header with language defaults, Base64 or Q transfer, and folded lines."
---

## mb_encode_mimeheader()

```php
function mb_encode_mimeheader(string $string, ?string $charset = null, ?string $transfer_encoding = null, string $newline = "\r\n", int $indent = 0): string
```

Encodes a MIME header with language defaults, Base64 or Q transfer, and folded lines.

**Parameters**:
- `$string` (`string`)
- `$charset` (`?string`), default `null`, optional
- `$transfer_encoding` (`?string`), default `null`, optional
- `$newline` (`string`), default `"\r\n"`, optional
- `$indent` (`int`), default `0`, optional

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_encode_mimeheader` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_encode_mimeheader.md).
