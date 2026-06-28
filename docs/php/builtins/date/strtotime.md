---
title: "strtotime()"
description: "Lowers `strtotime(datetime[, baseTimestamp])` through the shared parser runtime helper."
sidebar:
  order: 79
---

## strtotime()

```php
function strtotime(string $datetime, int $baseTimestamp): mixed
```

Lowers `strtotime(datetime[, baseTimestamp])` through the shared parser runtime helper.

**Parameters**:
- `$datetime` (`string`)
- `$baseTimestamp` (`int`), optional

**Returns**: `mixed`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `strtotime` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/strtotime.md).

