---
title: "random_bytes()"
description: "Generates cryptographically secure pseudo-random bytes."
---

## random_bytes()

```php
function random_bytes(int $length): string
```

Generates cryptographically secure pseudo-random bytes.

**Parameters**:
- `$length` (`int`)

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

- Compiled code accepts any argument count and raises PHP's `ArgumentCountError` at run time, so the signature shown is the PHP-visible one.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `random_bytes` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/random_bytes.md).
