---
title: "linkinfo()"
description: "Lowers `linkinfo(path)` through the target-aware runtime lstat helper."
sidebar:
  order: 115
---

## linkinfo()

```php
function linkinfo(string $path): int
```

Lowers `linkinfo(path)` through the target-aware runtime lstat helper.

**Parameters**:
- `$path` (`string`)

**Returns**: `int`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `linkinfo` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/linkinfo.md).

