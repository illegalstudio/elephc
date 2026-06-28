---
title: "mkdir()"
description: "Lowers `mkdir(path)` through the target-aware runtime helper."
sidebar:
  order: 117
---

## mkdir()

```php
function mkdir(string $directory, int $permissions, bool $recursive, bool $context): bool
```

Lowers `mkdir(path)` through the target-aware runtime helper.

**Parameters**:
- `$directory` (`string`)
- `$permissions` (`int`)
- `$recursive` (`bool`)
- `$context` (`bool`)

**Returns**: `bool`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `mkdir` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/mkdir.md).

