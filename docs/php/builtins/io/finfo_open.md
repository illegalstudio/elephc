---
title: "finfo_open()"
description: "Creates a finfo instance using the embedded or a custom magic database."
---

## finfo_open()

```php
function finfo_open(int $flags = 0, ?string $magic_database = null): finfo|false
```

Creates a finfo instance using the embedded or a custom magic database.

**Parameters**:
- `$flags` (`int`), default `0`, optional
- `$magic_database` (`?string`), default `null`, optional

**Returns**: `finfo|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `finfo_open` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/finfo_open.md).
