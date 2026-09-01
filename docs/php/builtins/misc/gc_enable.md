---
title: "gc_enable()"
description: "Enables the circular-reference collector."
sidebar:
  order: 327
---

## gc_enable()

```php
function gc_enable(): void
```

Enables the circular-reference collector.

**Parameters**: none.

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gc_enable` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/gc_enable.md).
