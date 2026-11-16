---
title: "gettimeofday()"
description: "Returns the current time as an array of parts, or as a float."
---

## gettimeofday()

```php
function gettimeofday(bool $as_float = false): mixed
```

Returns the current time as an array of parts, or as a float.

**Parameters**:
- `$as_float` (`bool`), default `false`, optional

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gettimeofday` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/gettimeofday.md).
