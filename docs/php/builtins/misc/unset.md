---
title: "unset()"
description: "Unsets the given variables."
---

## unset()

```php
function unset(mixed $var, ...$vars): void
```

Unsets the given variables.

**Parameters**:
- `$var` (`mixed`)
- `...$vars` - variadic: collects excess arguments into `$vars`.

**Returns**: `void`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `unset` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/unset.md).
