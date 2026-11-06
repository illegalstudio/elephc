---
title: "strcasecmp()"
description: "Binary safe case-insensitive string comparison. Returns negative, zero, or positive."
---

## strcasecmp()

```php
function strcasecmp(string $string1, string $string2): int
```

Binary safe case-insensitive string comparison. Returns negative, zero, or positive.

**Parameters**:
- `$string1` (`string`)
- `$string2` (`string`)

**Returns**: `int`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `strcasecmp` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/strcasecmp.md).
