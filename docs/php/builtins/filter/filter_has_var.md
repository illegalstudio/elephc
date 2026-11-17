---
title: "filter_has_var()"
description: "Checks whether a request input of the given type contains the named variable."
---

## filter_has_var()

```php
function filter_has_var(int $input_type, string $var_name): bool
```

Checks whether a request input of the given type contains the named variable.

**Parameters**:
- `$input_type` (`int`)
- `$var_name` (`string`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `filter_has_var` is implemented in the compiler, see [the internals page](../../../internals/builtins/filter/filter_has_var.md).
