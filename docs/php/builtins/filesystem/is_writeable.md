---
title: "is_writeable()"
description: "Tells whether the filename is writable (alias of is_writable)."
---

## is_writeable()

```php
function is_writeable(string $filename): bool
```

Tells whether the filename is writable (alias of is_writable).

**Parameters**:
- `$filename` (`string`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `is_writeable` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/is_writeable.md).
