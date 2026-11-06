---
title: "function_exists()"
description: "Returns true if the given function has been defined."
---

## function_exists()

```php
function function_exists(string $function): bool
```

Returns true if the given function has been defined.

**Parameters**:
- `$function` (`string`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `function_exists` is implemented in the compiler, see [the internals page](../../../internals/builtins/class/function_exists.md).
