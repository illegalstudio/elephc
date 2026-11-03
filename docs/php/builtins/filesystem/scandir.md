---
title: "scandir()"
description: "Lists files and directories inside the specified path."
sidebar:
  order: 316
---

## scandir()

```php
function scandir(string $directory, int $sorting_order = 0, mixed $context = null): array
```

Lists files and directories inside the specified path.

**Parameters**:
- `$directory` (`string`)
- `$sorting_order` (`int`), default `0`, optional
- `$context` (`mixed`), default `null`, optional

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/scandir.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/scandir.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `scandir` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/scandir.md).
