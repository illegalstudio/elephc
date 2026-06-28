---
title: "file_exists()"
description: "Lowers `file_exists(path)` through the target-aware runtime stat helper."
sidebar:
  order: 91
---

## file_exists()

```php
function file_exists(string $filename): bool
```

Lowers `file_exists(path)` through the target-aware runtime stat helper.

**Parameters**:
- `$filename` (`string`)

**Returns**: `bool`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `file_exists` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/file_exists.md).

