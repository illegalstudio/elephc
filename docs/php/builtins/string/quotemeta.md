---
title: "quotemeta()"
description: "Prefixes each regular-expression metacharacter in a string with a backslash."
---

## quotemeta()

```php
function quotemeta(string $string): string
```

Prefixes each regular-expression metacharacter in a string with a backslash.

**Parameters**:
- `$string` (`string`)

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `quotemeta` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/quotemeta.md).
