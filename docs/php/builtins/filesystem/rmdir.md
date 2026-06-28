---
title: "rmdir()"
description: "Lowers `rmdir(path)` through the target-aware runtime helper."
sidebar:
  order: 126
---

## rmdir()

```php
function rmdir(string $directory, mixed $context = null): bool
```

Lowers `rmdir(path)` through the target-aware runtime helper.

**Parameters**:
- `$directory` (`string`)
- `$context` (`mixed`), default `null`, optional

**Returns**: `bool`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `rmdir` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/rmdir.md).

