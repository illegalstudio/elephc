---
title: "mb_ereg_search()"
description: "Searches the retained multibyte subject and advances the shared byte position."
---

## mb_ereg_search()

```php
function mb_ereg_search(?string $pattern = null, ?string $options = null): bool
```

Searches the retained multibyte subject and advances the shared byte position.

**Parameters**:
- `$pattern` (`?string`), default `null`, optional
- `$options` (`?string`), default `null`, optional

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_ereg_search` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_ereg_search.md).
