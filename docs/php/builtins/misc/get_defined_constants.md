---
title: "get_defined_constants()"
description: "Returns constants visible to the current program."
---

## get_defined_constants()

```php
function get_defined_constants(bool $categorize = false): array
```

Returns constants visible to the current program.

**Parameters**:
- `$categorize` (`bool`), default `false`, optional

**Returns**: `array`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `get_defined_constants` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/get_defined_constants.md).
