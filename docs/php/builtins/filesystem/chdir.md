---
title: "chdir()"
description: "Lowers `chdir(path)` through the target-aware runtime helper."
sidebar:
  order: 82
---

## chdir()

```php
function chdir(string $directory): bool
```

Lowers `chdir(path)` through the target-aware runtime helper.

**Parameters**:
- `$directory` (`string`)

**Returns**: `bool`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `chdir` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/chdir.md).

