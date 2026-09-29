---
title: "ini_restore()"
description: "Restores a configuration directive to its startup value."
sidebar:
  order: 636
---

## ini_restore()

```php
function ini_restore(string $option): void
```

Restores a configuration directive to its startup value.

**Parameters**:
- `$option` (`string`)

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected version prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `ini_restore` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/ini_restore.md).
