---
title: "link()"
description: "Lowers `link(oldpath, newpath)` through the target-aware libc wrapper."
sidebar:
  order: 114
---

## link()

```php
function link(string $target, string $link): bool
```

Lowers `link(oldpath, newpath)` through the target-aware libc wrapper.

**Parameters**:
- `$target` (`string`)
- `$link` (`string`)

**Returns**: `bool`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `link` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/link.md).

