---
title: "umask()"
description: "Changes the current umask."
---

## umask()

```php
function umask(?int $mask = null): int
```

Changes the current umask.

**Parameters**:
- `$mask` (`?int`), default `null`, optional

**Returns**: `int`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `umask` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/umask.md).
