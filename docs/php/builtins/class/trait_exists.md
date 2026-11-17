---
title: "trait_exists()"
description: "Checks whether the trait exists."
---

## trait_exists()

```php
function trait_exists(string $trait, bool $autoload = true): bool
```

Checks whether the trait exists.

**Parameters**:
- `$trait` (`string`)
- `$autoload` (`bool`), default `true`, optional

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `trait_exists` is implemented in the compiler, see [the internals page](../../../internals/builtins/class/trait_exists.md).
