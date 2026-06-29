---
title: "ptr_sizeof()"
description: "Lowers `ptr_sizeof(\"type\")` by materializing the checked static byte size."
sidebar:
  order: 280
---

## ptr_sizeof()

```php
function ptr_sizeof(string $type): mixed
```

Lowers `ptr_sizeof("type")` by materializing the checked static byte size.

**Parameters**:
- `$type` (`string`)

**Returns**: `mixed`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `ptr_sizeof` is implemented in the compiler, see [the internals page](../../../internals/builtins/pointer/ptr_sizeof.md).

