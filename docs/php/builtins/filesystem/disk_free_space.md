---
title: "disk_free_space()"
description: "Lowers `disk_free_space(path)` through the shared disk-space runtime helper."
sidebar:
  order: 89
---

## disk_free_space()

```php
function disk_free_space(string $directory): float
```

Lowers `disk_free_space(path)` through the shared disk-space runtime helper.

**Parameters**:
- `$directory` (`string`)

**Returns**: `float`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `disk_free_space` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/disk_free_space.md).

