---
title: "mb_ereg_match()"
description: "Tests a raw multibyte regex at the string start using the current regex encoding and options."
---

## mb_ereg_match()

```php
function mb_ereg_match(string $pattern, string $string, ?string $options = null): bool
```

Tests a raw multibyte regex at the string start using the current regex encoding and options.

**Parameters**:
- `$pattern` (`string`)
- `$string` (`string`)
- `$options` (`?string`), default `null`, optional

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_ereg_match` is implemented in the compiler, see [the internals page](../../../internals/builtins/regex/mb_ereg_match.md).
