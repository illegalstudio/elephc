---
title: "rename()"
description: "Lowers `rename(from, to)` through the target-aware runtime helper."
sidebar:
  order: 125
---

## rename()

```php
function rename(string $from, string $to, mixed $context): bool
```

Lowers `rename(from, to)` through the target-aware runtime helper.

**Parameters**:
- `$from` (`string`)
- `$to` (`string`)
- `$context` (`mixed`)

**Returns**: `bool`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `rename` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/rename.md).

