---
title: "mb_decode_mimeheader()"
description: "Decodes MIME header words into the current internal encoding and unfolds whitespace."
---

## mb_decode_mimeheader()

```php
function mb_decode_mimeheader(string $string): string
```

Decodes MIME header words into the current internal encoding and unfolds whitespace.

**Parameters**:
- `$string` (`string`)

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_decode_mimeheader` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_decode_mimeheader.md).
