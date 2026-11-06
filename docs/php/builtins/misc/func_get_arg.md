---
title: "func_get_arg()"
description: "Returns one argument from the current function call."
---

## func_get_arg()

```php
function func_get_arg(int $position): mixed
```

Returns one argument from the current function call.

**Parameters**:
- `$position` (`int`)

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `func_get_arg` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/func_get_arg.md).
