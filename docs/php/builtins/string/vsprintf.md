---
title: "vsprintf()"
description: "Lowers `vsprintf(format, values)` through the array-to-sprintf runtime bridge."
sidebar:
  order: 386
---

## vsprintf()

```php
function vsprintf(string $format, array $values): string
```

Lowers `vsprintf(format, values)` through the array-to-sprintf runtime bridge.

**Parameters**:
- `$format` (`string`)
- `$values` (`array`)

**Returns**: `string`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `vsprintf` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/vsprintf.md).

