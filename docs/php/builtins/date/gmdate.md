---
title: "gmdate()"
description: "Lowers `gmdate(format[, timestamp])`: the UTC counterpart of `date()`."
sidebar:
  order: 73
---

## gmdate()

```php
function gmdate(string $format, int $timestamp): string
```

Lowers `gmdate(format[, timestamp])`: the UTC counterpart of `date()`.

**Parameters**:
- `$format` (`string`)
- `$timestamp` (`int`), optional

**Returns**: `string`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `gmdate` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/gmdate.md).

