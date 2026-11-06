---
title: "mb_language()"
description: "Reads or changes the current mbstring language."
---

## mb_language()

```php
function mb_language(?string $language = null): string|bool
```

Reads or changes the current mbstring language.

**Parameters**:
- `$language` (`?string`), default `null`, optional

**Returns**: `string|bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_language` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_language.md).
