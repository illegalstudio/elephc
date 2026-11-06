---
title: "clamp()"
description: "Clamps a value to be within a specified range."
---

## clamp()

```php
function clamp(int $value, int $min, int $max): mixed
```

Clamps a value to be within a specified range.

**Parameters**:
- `$value` (`int`)
- `$min` (`int`)
- `$max` (`int`)

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

- Hidden in strict PHP mode: this builtin is an elephc extension with no PHP equivalent, so programs compiled with [`--strict-php`](../../../compiling/cli-reference.md#strict-php-mode) treat the name as nonexistent, in compiled code and inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `clamp` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/clamp.md).
