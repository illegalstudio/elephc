---
title: "date()"
description: "Lowers `date(format, timestamp?)` through the shared formatter runtime helper."
sidebar:
  order: 69
---

## date()

```php
function date(string $format, int $timestamp): string
```

Lowers `date(format, timestamp?)` through the shared formatter runtime helper.

**Parameters**:
- `$format` (`string`)
- `$timestamp` (`int`), optional

**Returns**: `string`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `date` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/date.md).

