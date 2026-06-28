---
title: "fileperms()"
description: "Lowers `fileperms(path)` and boxes the runtime integer-or-false result."
sidebar:
  order: 98
---

## fileperms()

```php
function fileperms(string $filename): mixed
```

Lowers `fileperms(path)` and boxes the runtime integer-or-false result.

**Parameters**:
- `$filename` (`string`)

**Returns**: `mixed`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `fileperms` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/fileperms.md).

