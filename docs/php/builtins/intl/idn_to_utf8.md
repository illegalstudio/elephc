---
title: "idn_to_utf8()"
description: "PHP ext/intl `idn_to_utf8()` backed by the managed ICU bridge."
---

## idn_to_utf8()

```php
function idn_to_utf8(string $domain, int $flags = IDNA_DEFAULT, int $variant = INTL_IDNA_VARIANT_UTS46, mixed $idna_info = null): string|false
```

PHP ext/intl `idn_to_utf8()` backed by the managed ICU bridge.

**Parameters**:
- `$domain` (`string`)
- `$flags` (`int`), default `IDNA_DEFAULT`, optional
- `$variant` (`int`), default `INTL_IDNA_VARIANT_UTS46`, optional
- `$idna_info` (`mixed`), passed by reference, default `null`, optional

**Returns**: `string|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `idn_to_utf8` is implemented in the compiler, see [the internals page](../../../internals/builtins/intl/idn_to_utf8.md).
