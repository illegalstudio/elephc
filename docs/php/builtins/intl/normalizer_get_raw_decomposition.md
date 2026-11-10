---
title: "normalizer_get_raw_decomposition()"
description: "PHP ext/intl `normalizer_get_raw_decomposition()` backed by the managed ICU bridge."
---

## normalizer_get_raw_decomposition()

```php
function normalizer_get_raw_decomposition(string $string, int $form = Normalizer::FORM_C): ?string
```

PHP ext/intl `normalizer_get_raw_decomposition()` backed by the managed ICU bridge.

**Parameters**:
- `$string` (`string`)
- `$form` (`int`), default `Normalizer::FORM_C`, optional

**Returns**: `?string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `normalizer_get_raw_decomposition` is implemented in the compiler, see [the internals page](../../../internals/builtins/intl/normalizer_get_raw_decomposition.md).
