---
title: "ptr_write16()"
description: "Lowers `ptr_write16(pointer, value)` by writing one 16-bit word through a checked pointer."
sidebar:
  order: 279
---

## ptr_write16()

```php
function ptr_write16(pointer $pointer, int $value): void
```

Lowers `ptr_write16(pointer, value)` by writing one 16-bit word through a checked pointer.

**Parameters**:
- `$pointer` (`pointer`)
- `$value` (`int`)

**Returns**: `void`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `ptr_write16` is implemented in the compiler, see [the internals page](../../../internals/builtins/pointer/ptr_write16.md).

