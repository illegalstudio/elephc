---
title: "chmod()"
description: "Lowers `chmod(path, mode)` through the target-aware runtime helper."
sidebar:
  order: 84
---

## chmod()

```php
function chmod(string $filename, int $permissions): bool
```

Lowers `chmod(path, mode)` through the target-aware runtime helper.

**Parameters**:
- `$filename` (`string`)
- `$permissions` (`int`)

**Returns**: `bool`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `chmod` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/chmod.md).

