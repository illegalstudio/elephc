---
title: "mb_check_encoding()"
description: "Checks encoded strings or array keys and values recursively for invalid byte sequences."
---

## mb_check_encoding()

```php
function mb_check_encoding(array|string|null $value = null, ?string $encoding = null): bool
```

Checks encoded strings or array keys and values recursively for invalid byte sequences.

**Parameters**:
- `$value` (`array|string|null`), default `null`, optional
- `$encoding` (`?string`), default `null`, optional

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_check_encoding` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_check_encoding.md).
