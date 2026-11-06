---
title: "readdir()"
description: "Read entry from directory handle."
---

## readdir()

```php
function readdir(resource $dir_handle): mixed
```

Read entry from directory handle.

**Parameters**:
- `$dir_handle` (`resource`)

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `readdir` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/readdir.md).
