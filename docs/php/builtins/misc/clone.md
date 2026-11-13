---
title: "clone()"
description: "Creates a shallow clone, then initializes selected properties on the clone."
---

## clone()

```php
function clone(object $object, array $withProperties = []): object
```

Creates a shallow clone, then initializes selected properties on the clone.

**Parameters**:
- `$object` (`object`)
- `$withProperties` (`array`), default `[]`, optional

**Returns**: `object`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `clone` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/clone.md).
