---
title: "fileinode()"
description: "Lowers `fileinode(path)` and boxes the runtime integer-or-false result."
sidebar:
  order: 95
---

## fileinode()

```php
function fileinode(string $filename): mixed
```

Lowers `fileinode(path)` and boxes the runtime integer-or-false result.

**Parameters**:
- `$filename` (`string`)

**Returns**: `mixed`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `fileinode` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/fileinode.md).

