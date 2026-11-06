---
title: "log2()"
description: "Returns the base-2 logarithm of a number."
---

## log2()

```php
function log2(float $num): float
```

Returns the base-2 logarithm of a number.

**Parameters**:
- `$num` (`float`)

**Returns**: `float`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

- Hidden in strict PHP mode: this builtin is an elephc extension with no PHP equivalent, so programs compiled with [`--strict-php`](../../../compiling/cli-reference.md#strict-php-mode) treat the name as nonexistent, in compiled code and inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `log2` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/log2.md).
