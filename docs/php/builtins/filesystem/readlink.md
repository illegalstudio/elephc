---
title: "readlink()"
description: "Lowers `readlink(path)` and boxes the owned runtime string-or-false result."
sidebar:
  order: 121
---

## readlink()

```php
function readlink(string $path): mixed
```

Lowers `readlink(path)` and boxes the owned runtime string-or-false result.

**Parameters**:
- `$path` (`string`)

**Returns**: `mixed`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `readlink` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/readlink.md).

