---
title: "gc_collect_cycles()"
description: "Forces collection of existing garbage cycles."
sidebar:
  order: 326
---

## gc_collect_cycles()

```php
function gc_collect_cycles(): int
```

Forces collection of existing garbage cycles.

**Parameters**: none.

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gc_collect_cycles` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/gc_collect_cycles.md).
