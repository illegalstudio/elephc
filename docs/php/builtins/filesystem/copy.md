---
title: "copy()"
description: "Lowers `copy(source, dest)` through the target-aware runtime helper."
sidebar:
  order: 87
---

## copy()

```php
function copy(string $from, string $to, mixed $context): bool
```

Lowers `copy(source, dest)` through the target-aware runtime helper.

**Parameters**:
- `$from` (`string`)
- `$to` (`string`)
- `$context` (`mixed`)

**Returns**: `bool`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `copy` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/copy.md).

