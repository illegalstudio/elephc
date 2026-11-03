---
title: "collator_create()"
description: "PHP ext/intl `collator_create()` backed by the managed ICU bridge."
---

## collator_create()

```php
function collator_create(string $locale): ?object
```

PHP ext/intl `collator_create()` backed by the managed ICU bridge.

**Parameters**:
- `$locale` (`string`)

**Returns**: `?object`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `collator_create` is implemented in the compiler, see [the internals page](../../../internals/builtins/intl/collator_create.md).
