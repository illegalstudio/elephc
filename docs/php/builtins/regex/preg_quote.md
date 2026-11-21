---
title: "preg_quote()"
description: "Escapes the regex metacharacters of `$str` with a backslash, plus the first byte of `$delimiter` when given; NUL becomes `\000`."
---

## preg_quote()

```php
function preg_quote(string $str, ?string $delimiter = null): string
```

Escapes the regex metacharacters of `$str` with a backslash, plus the first byte of `$delimiter` when given; NUL becomes `\000`.

**Parameters**:
- `$str` (`string`)
- `$delimiter` (`?string`), default `null`, optional

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `preg_quote` is implemented in the compiler, see [the internals page](../../../internals/builtins/regex/preg_quote.md).
