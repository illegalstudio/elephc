---
title: "iconv_substr()"
description: "Extracts a character-indexed slice of a string."
---

## iconv_substr()

```php
function iconv_substr(string $string, int $offset, ?int $length = null, ?string $encoding = null): mixed
```

Extracts a character-indexed slice of a string.

**Parameters**:
- `$string` (`string`)
- `$offset` (`int`)
- `$length` (`?int`), default `null`, optional
- `$encoding` (`?string`), default `null`, optional

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `iconv_substr` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/iconv_substr.md).
