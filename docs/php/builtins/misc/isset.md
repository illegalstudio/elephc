---
title: "isset()"
description: "Determines whether a variable is set and is not null."
---

## isset()

```php
function isset(mixed $var, ...$vars): bool
```

Determines whether a variable is set and is not null.

**Parameters**:
- `$var` (`mixed`)
- `...$vars` - variadic: collects excess arguments into `$vars`.

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `isset` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/isset.md).
