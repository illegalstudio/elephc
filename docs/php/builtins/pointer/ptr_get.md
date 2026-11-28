---
title: "ptr_get()"
description: "Reads one machine word through a raw pointer and returns it as an integer."
---

## ptr_get()

```php
function ptr_get(pointer $pointer): int
```

Reads one machine word through a raw pointer and returns it as an integer.

**Parameters**:
- `$pointer` (`pointer`)

**Returns**: `int`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

- Hidden in PHP-only mode: this builtin is an elephc extension with no PHP equivalent, so programs compiled with [`--php-only`](../../../compiling/cli-reference.md#php-only-mode) treat the name as nonexistent, in compiled code and inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `ptr_get` is implemented in the compiler, see [the internals page](../../../internals/builtins/pointer/ptr_get.md).
