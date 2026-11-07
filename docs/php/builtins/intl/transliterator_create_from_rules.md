---
title: "transliterator_create_from_rules()"
description: "PHP ext/intl `transliterator_create_from_rules()` backed by the managed ICU bridge."
---

## transliterator_create_from_rules()

```php
function transliterator_create_from_rules(string $rules, int $direction = Transliterator::FORWARD): ?object
```

PHP ext/intl `transliterator_create_from_rules()` backed by the managed ICU bridge.

**Parameters**:
- `$rules` (`string`)
- `$direction` (`int`), default `Transliterator::FORWARD`, optional

**Returns**: `?object`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `transliterator_create_from_rules` is implemented in the compiler, see [the internals page](../../../internals/builtins/intl/transliterator_create_from_rules.md).
