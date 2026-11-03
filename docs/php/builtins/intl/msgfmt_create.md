---
title: "msgfmt_create()"
description: "PHP ext/intl `msgfmt_create()` backed by the managed ICU bridge."
---

## msgfmt_create()

```php
function msgfmt_create(string $locale, string $pattern): ?object
```

PHP ext/intl `msgfmt_create()` backed by the managed ICU bridge.

**Parameters**:
- `$locale` (`string`)
- `$pattern` (`string`)

**Returns**: `?object`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `msgfmt_create` is implemented in the compiler, see [the internals page](../../../internals/builtins/intl/msgfmt_create.md).
