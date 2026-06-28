---
title: "dirname()"
description: "Lowers `dirname(path, levels?)` through the target-aware runtime helper."
sidebar:
  order: 88
---

## dirname()

```php
function dirname(string $path, int $levels): string
```

Lowers `dirname(path, levels?)` through the target-aware runtime helper.

**Parameters**:
- `$path` (`string`)
- `$levels` (`int`), optional

**Returns**: `string`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `dirname` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/dirname.md).

