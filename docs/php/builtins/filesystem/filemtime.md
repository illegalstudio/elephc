---
title: "filemtime()"
description: "Lowers `filemtime(path)` through the target-aware runtime stat helper."
sidebar:
  order: 96
---

## filemtime()

```php
function filemtime(string $filename): int
```

Lowers `filemtime(path)` through the target-aware runtime stat helper.

**Parameters**:
- `$filename` (`string`)

**Returns**: `int`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `filemtime` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/filemtime.md).

