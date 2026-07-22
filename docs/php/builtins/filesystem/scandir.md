---
title: "scandir()"
description: "Lists files and directories inside the specified path."
sidebar:
  order: 150
---

## scandir()

```php
function scandir(string $directory): array
```

Lists files and directories inside the specified path.

**Parameters**:
- `$directory` (`string`)

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/scandir.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/scandir.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `scandir` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/scandir.md).
