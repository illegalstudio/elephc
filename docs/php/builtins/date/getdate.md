---
title: "getdate()"
description: "Lowers `getdate([$timestamp])` through the shared decomposition runtime helper."
sidebar:
  order: 72
---

## getdate()

```php
function getdate(int $timestamp): array
```

Lowers `getdate([$timestamp])` through the shared decomposition runtime helper.

**Parameters**:
- `$timestamp` (`int`), optional

**Returns**: `array`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `getdate` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/getdate.md).

