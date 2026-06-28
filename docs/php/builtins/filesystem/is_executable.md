---
title: "is_executable()"
description: "Lowers `is_executable(path)` through the target-aware runtime access helper."
sidebar:
  order: 106
---

## is_executable()

```php
function is_executable(string $filename): bool
```

Lowers `is_executable(path)` through the target-aware runtime access helper.

**Parameters**:
- `$filename` (`string`)

**Returns**: `bool`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `is_executable` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/is_executable.md).

