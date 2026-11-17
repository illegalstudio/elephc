---
title: "iconv_strpos()"
description: "Finds the first character position of a needle in a string."
---

## iconv_strpos()

```php
function iconv_strpos(string $haystack, string $needle, int $offset = 0, ?string $encoding = null): mixed
```

Finds the first character position of a needle in a string.

**Parameters**:
- `$haystack` (`string`)
- `$needle` (`string`)
- `$offset` (`int`), default `0`, optional
- `$encoding` (`?string`), default `null`, optional

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `iconv_strpos` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/iconv_strpos.md).
