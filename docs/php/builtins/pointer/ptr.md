---
title: "ptr()"
description: "Lowers `ptr(value)` by materializing the address of addressable local/global storage."
sidebar:
  order: 270
---

## ptr()

```php
function ptr(mixed $value): mixed
```

Lowers `ptr(value)` by materializing the address of addressable local/global storage.

**Parameters**:
- `$value` (`mixed`)

**Returns**: `mixed`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `ptr` is implemented in the compiler, see [the internals page](../../../internals/builtins/pointer/ptr.md).

