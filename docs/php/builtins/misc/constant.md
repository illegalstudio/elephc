---
title: "constant()"
description: "Returns the value of a constant given its name."
---

## constant()

```php
function constant(string $name): mixed
```

Returns the value of a constant given its name.

**Parameters**:
- `$name` (`string`)

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `constant` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/constant.md).
