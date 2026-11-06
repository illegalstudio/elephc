---
title: "strcmp()"
description: "Binary safe string comparison. Returns negative, zero, or positive."
---

## strcmp()

```php
function strcmp(string $string1, string $string2): int
```

Binary safe string comparison. Returns negative, zero, or positive.

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

For how `strcmp` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/strcmp.md).
