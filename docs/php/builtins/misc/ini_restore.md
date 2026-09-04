---
title: "ini_restore()"
description: "Implemented by the compiler-injected version prelude."
sidebar:
  order: 610
---

## ini_restore()

```php
function ini_restore(string $option): void
```

Implemented by the compiler-injected version prelude.

**Parameters**:
- `$option` (`string`)

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected version prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `ini_restore` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/ini_restore.md).
