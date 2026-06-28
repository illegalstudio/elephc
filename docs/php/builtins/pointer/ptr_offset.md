---
title: "ptr_offset()"
description: "Lowers `ptr_offset(pointer, offset)` by adding a byte offset to a raw address."
sidebar:
  order: 272
---

## ptr_offset()

```php
function ptr_offset(pointer $pointer, int $offset): mixed
```

Lowers `ptr_offset(pointer, offset)` by adding a byte offset to a raw address.

**Parameters**:
- `$pointer` (`pointer`)
- `$offset` (`int`)

**Returns**: `mixed`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `ptr_offset` is implemented in the compiler, see [the internals page](../../../internals/builtins/pointer/ptr_offset.md).

