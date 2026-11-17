---
title: "copy()"
description: "Copies a file."
---

## copy()

```php
function copy(string $from, string $to): bool
```

Copies a file.

**Parameters**:
- `$from` (`string`)
- `$to` (`string`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `copy` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/copy.md).
