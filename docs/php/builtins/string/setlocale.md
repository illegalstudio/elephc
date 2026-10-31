---
title: "setlocale()"
description: "Sets locale information and returns the new locale name, or false when unavailable."
sidebar:
  order: 985
---

## setlocale()

```php
function setlocale(int $category, mixed $locales, ...$rest): string|false
```

Sets locale information and returns the new locale name, or false when unavailable.

**Parameters**:
- `$category` (`int`)
- `$locales` (`mixed`)
- `...$rest` - variadic: collects excess arguments into `$rest`.

**Returns**: `string|false`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected locale prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `setlocale` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/setlocale.md).
