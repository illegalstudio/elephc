---
title: "usleep()"
description: "Lowers `usleep(microseconds)` through the target's C library symbol."
sidebar:
  order: 310
---

## usleep()

```php
function usleep(int $microseconds): void
```

Lowers `usleep(microseconds)` through the target's C library symbol.

**Parameters**:
- `$microseconds` (`int`)

**Returns**: `void`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `usleep` is implemented in the compiler, see [the internals page](../../../internals/builtins/process/usleep.md).

