---
title: "ptr_offset()"
description: "Returns a new pointer offset from the given pointer by the given byte count."
---

## ptr_offset()

```php
function ptr_offset(pointer $pointer, int $offset): pointer
```

Returns a new pointer offset from the given pointer by the given byte count.

**Parameters**:
- `$pointer` (`pointer`)
- `$offset` (`int`)

**Returns**: `pointer`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

- Hidden in strict PHP mode: this builtin is an elephc extension with no PHP equivalent, so programs compiled with [`--strict-php`](../../../compiling/cli-reference.md#strict-php-mode) treat the name as nonexistent, in compiled code and inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `ptr_offset` is implemented in the compiler, see [the internals page](../../../internals/builtins/pointer/ptr_offset.md).
