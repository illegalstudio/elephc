---
title: "normalizer_normalize()"
description: "PHP ext/intl `normalizer_normalize()` backed by the managed ICU bridge."
---

## normalizer_normalize()

```php
function normalizer_normalize(string $string, int $form = Normalizer::FORM_C): string|false
```

PHP ext/intl `normalizer_normalize()` backed by the managed ICU bridge.

**Parameters**:
- `$string` (`string`)
- `$form` (`int`), default `Normalizer::FORM_C`, optional

**Returns**: `string|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `normalizer_normalize` is implemented in the compiler, see [the internals page](../../../internals/builtins/intl/normalizer_normalize.md).
