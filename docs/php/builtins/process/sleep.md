---
title: "sleep()"
description: "Lowers `sleep(seconds)` through the target's C library symbol."
sidebar:
  order: 291
---

## sleep()

```php
function sleep(int $seconds): int
```

Lowers `sleep(seconds)` through the target's C library symbol.

**Parameters**:
- `$seconds` (`int`)

**Returns**: `int`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `sleep` is implemented in the compiler, see [the internals page](../../../internals/builtins/process/sleep.md).

