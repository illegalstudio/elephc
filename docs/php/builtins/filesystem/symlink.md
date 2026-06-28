---
title: "symlink()"
description: "Lowers `symlink(target, link)` through the target-aware libc wrapper."
sidebar:
  order: 129
---

## symlink()

```php
function symlink(string $target, string $link): bool
```

Lowers `symlink(target, link)` through the target-aware libc wrapper.

**Parameters**:
- `$target` (`string`)
- `$link` (`string`)

**Returns**: `bool`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `symlink` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/symlink.md).

