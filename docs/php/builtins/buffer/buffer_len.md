---
title: "buffer_len()"
description: "Returns the logical element count of a buffer<T>."
---

## buffer_len()

```php
function buffer_len(buffer $buffer): int
```

Returns the logical element count of a buffer<T>.

**Parameters**:
- `$buffer` (`buffer`)

**Returns**: `int`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

- Hidden in PHP-only mode: this builtin is an elephc extension with no PHP equivalent, so programs compiled with [`--php-only`](../../../compiling/cli-reference.md#php-only-mode) treat the name as nonexistent, in compiled code and inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `buffer_len` is implemented in the compiler, see [the internals page](../../../internals/builtins/buffer/buffer_len.md).
