---
title: "ptr_write16()"
description: "Writes one 16-bit word through a raw pointer."
---

## ptr_write16()

```php
function ptr_write16(pointer $pointer, int $value): void
```

Writes one 16-bit word through a raw pointer.

**Parameters**:
- `$pointer` (`pointer`)
- `$value` (`int`)

**Returns**: `void`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

- Hidden in strict PHP mode: this builtin is an elephc extension with no PHP equivalent, so programs compiled with [`--strict-php`](../../../compiling/cli-reference.md#strict-php-mode) treat the name as nonexistent, in compiled code and inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `ptr_write16` is implemented in the compiler, see [the internals page](../../../internals/builtins/pointer/ptr_write16.md).
