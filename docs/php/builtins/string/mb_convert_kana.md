---
title: "mb_convert_kana()"
description: "Converts Japanese width and kana according to the requested mode flags."
---

## mb_convert_kana()

```php
function mb_convert_kana(string $string, string $mode = 'KV', ?string $encoding = null): string
```

Converts Japanese width and kana according to the requested mode flags.

**Parameters**:
- `$string` (`string`)
- `$mode` (`string`), default `'KV'`, optional
- `$encoding` (`?string`), default `null`, optional

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_convert_kana` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_convert_kana.md).
