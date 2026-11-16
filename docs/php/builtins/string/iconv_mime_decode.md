---
title: "iconv_mime_decode()"
description: "Decodes one MIME header field into the requested character encoding."
---

## iconv_mime_decode()

```php
function iconv_mime_decode(string $string, int $mode = 0, ?string $encoding = null): mixed
```

Decodes one MIME header field into the requested character encoding.

**Parameters**:
- `$string` (`string`)
- `$mode` (`int`), default `0`, optional
- `$encoding` (`?string`), default `null`, optional

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `iconv_mime_decode` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/iconv_mime_decode.md).
