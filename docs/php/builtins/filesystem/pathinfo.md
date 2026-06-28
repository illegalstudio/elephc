---
title: "pathinfo()"
description: "Lowers `pathinfo(path, flags?)` through string, array, or boxed dynamic helpers."
sidebar:
  order: 118
---

## pathinfo()

```php
function pathinfo(string $path, int $flags): mixed
```

Lowers `pathinfo(path, flags?)` through string, array, or boxed dynamic helpers.

**Parameters**:
- `$path` (`string`)
- `$flags` (`int`), optional

**Returns**: `mixed`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `pathinfo` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/pathinfo.md).

