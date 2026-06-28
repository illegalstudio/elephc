---
title: "is_dir()"
description: "Lowers `is_dir(path)` through the target-aware runtime stat helper."
sidebar:
  order: 105
---

## is_dir()

```php
function is_dir(string $filename): bool
```

Lowers `is_dir(path)` through the target-aware runtime stat helper.

**Parameters**:
- `$filename` (`string`)

**Returns**: `bool`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `is_dir` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/is_dir.md).

