---
title: "setlocale()"
description: "Sets the process locale from one or more candidates and returns the active locale, or false on failure."
---

## setlocale()

```php
function setlocale(int $category, mixed $locales, ...$rest): string|false
```

Sets the process locale from one or more candidates and returns the active locale, or false on failure.

**Parameters**:
- `$category` (`int`)
- `$locales` (`mixed`)
- `...$rest` - variadic: collects excess arguments into `$rest`.

**Returns**: `string|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `setlocale` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/setlocale.md).
