---
title: "ptr_write8()"
description: "Lowers `ptr_write8(pointer, value)` by writing one byte through a checked pointer."
sidebar:
  order: 283
---

## ptr_write8()

```php
function ptr_write8(pointer $pointer, int $value): void
```

Lowers `ptr_write8(pointer, value)` by writing one byte through a checked pointer.

**Parameters**:
- `$pointer` (`pointer`)
- `$value` (`int`)

**Returns**: `void`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `ptr_write8` is implemented in the compiler, see [the internals page](../../../internals/builtins/pointer/ptr_write8.md).

