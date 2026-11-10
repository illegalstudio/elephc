---
title: "normalizer_is_normalized()"
description: "PHP ext/intl `normalizer_is_normalized()` backed by the managed ICU bridge."
---

## normalizer_is_normalized()

```php
function normalizer_is_normalized(string $string, int $form = Normalizer::FORM_C): bool
```

PHP ext/intl `normalizer_is_normalized()` backed by the managed ICU bridge.

**Parameters**:
- `$string` (`string`)
- `$form` (`int`), default `Normalizer::FORM_C`, optional

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `normalizer_is_normalized` is implemented in the compiler, see [the internals page](../../../internals/builtins/intl/normalizer_is_normalized.md).
