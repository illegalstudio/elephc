---
title: "mb_eregi_replace()"
description: "Replaces multibyte regex matches without case sensitivity."
---

## mb_eregi_replace()

```php
function mb_eregi_replace(string $pattern, string $replacement, string $string, ?string $options = null): string|false|null
```

Replaces multibyte regex matches without case sensitivity.

**Parameters**:
- `$pattern` (`string`)
- `$replacement` (`string`)
- `$string` (`string`)
- `$options` (`?string`), default `null`, optional

**Returns**: `string|false|null`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_eregi_replace` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_eregi_replace.md).
