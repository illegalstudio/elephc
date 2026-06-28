---
title: "touch()"
description: "Lowers `touch(path, mtime?, atime?)` through the target-aware runtime helper."
sidebar:
  order: 133
---

## touch()

```php
function touch(string $filename, int $mtime, int $atime): bool
```

Lowers `touch(path, mtime?, atime?)` through the target-aware runtime helper.

**Parameters**:
- `$filename` (`string`)
- `$mtime` (`int`), optional
- `$atime` (`int`), optional

**Returns**: `bool`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `touch` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/touch.md).

