---
title: "microtime()"
description: "Returns the current Unix timestamp with microseconds."
---

## microtime()

```php
function microtime(bool $as_float = false): mixed
```

Returns the current Unix timestamp with microseconds.

**Parameters**:
- `$as_float` (`bool`), default `false`, optional

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `microtime` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/microtime.md).
