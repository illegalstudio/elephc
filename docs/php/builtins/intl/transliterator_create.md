---
title: "transliterator_create()"
description: "PHP ext/intl `transliterator_create()` backed by the managed ICU bridge."
---

## transliterator_create()

```php
function transliterator_create(string $id, int $direction = Transliterator::FORWARD): ?object
```

PHP ext/intl `transliterator_create()` backed by the managed ICU bridge.

**Parameters**:
- `$id` (`string`)
- `$direction` (`int`), default `Transliterator::FORWARD`, optional

**Returns**: `?object`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `transliterator_create` is implemented in the compiler, see [the internals page](../../../internals/builtins/intl/transliterator_create.md).
