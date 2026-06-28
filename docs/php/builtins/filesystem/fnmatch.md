---
title: "fnmatch()"
description: "Lowers `fnmatch(pattern, filename, flags?)` through the target-aware runtime helper."
sidebar:
  order: 101
---

## fnmatch()

```php
function fnmatch(string $pattern, string $filename, int $flags): bool
```

Lowers `fnmatch(pattern, filename, flags?)` through the target-aware runtime helper.

**Parameters**:
- `$pattern` (`string`)
- `$filename` (`string`)
- `$flags` (`int`), optional

**Returns**: `bool`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `fnmatch` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/fnmatch.md).

