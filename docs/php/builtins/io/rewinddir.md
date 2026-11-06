---
title: "rewinddir()"
description: "Rewind directory handle."
---

## rewinddir()

```php
function rewinddir(resource $dir_handle): void
```

Rewind directory handle.

**Parameters**:
- `$dir_handle` (`resource`)

**Returns**: `void`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `rewinddir` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/rewinddir.md).
