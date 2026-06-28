---
title: "stat()"
description: "Lowers `stat(path)` and boxes the runtime stat array or PHP false result."
sidebar:
  order: 128
---

## stat()

```php
function stat(string $filename): mixed
```

Lowers `stat(path)` and boxes the runtime stat array or PHP false result.

**Parameters**:
- `$filename` (`string`)

**Returns**: `mixed`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `stat` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/stat.md).

