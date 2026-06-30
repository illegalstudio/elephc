---
title: "ptr_write32()"
description: "Lowers `ptr_write32(pointer, value)` by writing one 32-bit word through a checked pointer."
sidebar:
  order: 297
---

## ptr_write32()

```php
function ptr_write32(pointer $pointer, int $value): void
```

Lowers `ptr_write32(pointer, value)` by writing one 32-bit word through a checked pointer.

**Parameters**:
- `$pointer` (`pointer`)
- `$value` (`int`)

**Returns**: `void`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `ptr_write32` is implemented in the compiler, see [the internals page](../../../internals/builtins/pointer/ptr_write32.md).

