---
title: "localtime()"
description: "Lowers `localtime([$timestamp[, $associative]])` through the shared decomposition runtime helper."
sidebar:
  order: 76
---

## localtime()

```php
function localtime(int $timestamp, bool $associative): array
```

Lowers `localtime([$timestamp[, $associative]])` through the shared decomposition runtime helper.

**Parameters**:
- `$timestamp` (`int`), optional
- `$associative` (`bool`), optional

**Returns**: `array`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `localtime` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/localtime.md).

