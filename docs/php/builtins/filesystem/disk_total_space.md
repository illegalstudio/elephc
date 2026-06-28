---
title: "disk_total_space()"
description: "Lowers `disk_total_space(path)` through the shared disk-space runtime helper."
sidebar:
  order: 90
---

## disk_total_space()

```php
function disk_total_space(string $directory): float
```

Lowers `disk_total_space(path)` through the shared disk-space runtime helper.

**Parameters**:
- `$directory` (`string`)

**Returns**: `float`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `disk_total_space` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/disk_total_space.md).

