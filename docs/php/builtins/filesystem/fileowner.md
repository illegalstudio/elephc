---
title: "fileowner()"
description: "Lowers `fileowner(path)` and boxes the runtime integer-or-false result."
sidebar:
  order: 97
---

## fileowner()

```php
function fileowner(string $filename): mixed
```

Lowers `fileowner(path)` and boxes the runtime integer-or-false result.

**Parameters**:
- `$filename` (`string`)

**Returns**: `mixed`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `fileowner` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/fileowner.md).

