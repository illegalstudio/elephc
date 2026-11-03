---
title: "is_real()"
description: "Alias of is_float()."
---

## is_real()

```php
function is_real(mixed $value): bool
```

Alias of is_float().

**Parameters**:
- `$value` (`mixed`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

- Hidden in strict PHP mode: this builtin is an elephc extension with no PHP equivalent, so programs compiled with [`--strict-php`](../../../compiling/cli-reference.md#strict-php-mode) treat the name as nonexistent, in compiled code and inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `is_real` is implemented in the compiler, see [the internals page](../../../internals/builtins/type/is_real.md).
