---
title: "closedir()"
description: "Closes directory handle."
---

## closedir()

```php
function closedir(resource $dir_handle): void
```

Closes directory handle.

**Parameters**:
- `$dir_handle` (`resource`)

**Returns**: `void`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `closedir` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/closedir.md).
