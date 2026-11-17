---
title: "ptr_null()"
description: "Returns a null raw pointer."
---

## ptr_null()

```php
function ptr_null(): pointer
```

Returns a null raw pointer.

**Parameters**: none.

**Returns**: `pointer`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

- Hidden in strict PHP mode: this builtin is an elephc extension with no PHP equivalent, so programs compiled with [`--strict-php`](../../../compiling/cli-reference.md#strict-php-mode) treat the name as nonexistent, in compiled code and inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `ptr_null` is implemented in the compiler, see [the internals page](../../../internals/builtins/pointer/ptr_null.md).
