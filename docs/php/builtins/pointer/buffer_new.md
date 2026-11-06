---
title: "buffer_new()"
description: "Allocates a raw byte buffer."
---

## buffer_new()

```php
function buffer_new(int $length): mixed
```

Allocates a raw byte buffer.

**Parameters**:
- `$length` (`int`)

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

- Hidden in strict PHP mode: this builtin is an elephc extension with no PHP equivalent, so programs compiled with [`--strict-php`](../../../compiling/cli-reference.md#strict-php-mode) treat the name as nonexistent, in compiled code and inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `buffer_new` is implemented in the compiler, see [the internals page](../../../internals/builtins/pointer/buffer_new.md).
