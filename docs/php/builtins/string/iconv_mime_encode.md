---
title: "iconv_mime_encode()"
description: "Encodes one header field as RFC 2047 encoded-words."
---

## iconv_mime_encode()

```php
function iconv_mime_encode(string $field_name, string $field_value, mixed $options = []): mixed
```

Encodes one header field as RFC 2047 encoded-words.

**Parameters**:
- `$field_name` (`string`)
- `$field_value` (`string`)
- `$options` (`mixed`), default `[]`, optional

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `iconv_mime_encode` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/iconv_mime_encode.md).
