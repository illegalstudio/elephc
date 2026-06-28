---
title: "basename()"
description: "Lowers `basename(path, suffix?)` through the target-aware runtime helper."
sidebar:
  order: 81
---

## basename()

```php
function basename(string $path, string $suffix): string
```

Lowers `basename(path, suffix?)` through the target-aware runtime helper.

**Parameters**:
- `$path` (`string`)
- `$suffix` (`string`), optional

**Returns**: `string`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `basename` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/basename.md).

