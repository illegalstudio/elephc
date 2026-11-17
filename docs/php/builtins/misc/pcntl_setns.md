---
title: "pcntl_setns()"
description: "Joins one Linux namespace of the selected process."
---

## pcntl_setns()

```php
function pcntl_setns(int $process_id = null, int $nstype = 1073741824): bool
```

Joins one Linux namespace of the selected process.

**Parameters**:
- `$process_id` (`int`), default `null`, optional
- `$nstype` (`int`), default `1073741824`, optional

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `pcntl_setns` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/pcntl_setns.md).
