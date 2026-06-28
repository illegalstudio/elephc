---
title: "lstat()"
description: "Lowers `lstat(path)` and boxes the runtime lstat array or PHP false result."
sidebar:
  order: 116
---

## lstat()

```php
function lstat(string $filename): mixed
```

Lowers `lstat(path)` and boxes the runtime lstat array or PHP false result.

**Parameters**:
- `$filename` (`string`)

**Returns**: `mixed`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `lstat` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/lstat.md).

