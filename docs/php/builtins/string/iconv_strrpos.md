---
title: "iconv_strrpos()"
description: "Finds the last character position of a needle in a string."
---

## iconv_strrpos()

```php
function iconv_strrpos(string $haystack, string $needle, ?string $encoding = null): mixed
```

Finds the last character position of a needle in a string.

**Parameters**:
- `$haystack` (`string`)
- `$needle` (`string`)
- `$encoding` (`?string`), default `null`, optional

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `iconv_strrpos` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/iconv_strrpos.md).
