---
title: "filesize()"
description: "Lowers `filesize(path)` through the target-aware runtime stat helper."
sidebar:
  order: 99
---

## filesize()

```php
function filesize(string $filename): int
```

Lowers `filesize(path)` through the target-aware runtime stat helper.

**Parameters**:
- `$filename` (`string`)

**Returns**: `int`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `filesize` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/filesize.md).

