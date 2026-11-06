---
title: "gettype()"
description: "Returns the type of a variable as a string."
---

## gettype()

```php
function gettype(mixed $value): string
```

Returns the type of a variable as a string.

**Parameters**:
- `$value` (`mixed`)

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gettype` is implemented in the compiler, see [the internals page](../../../internals/builtins/type/gettype.md).
