---
title: "tempnam()"
description: "Creates a file with a unique filename."
sidebar:
  order: 161
---

## tempnam()

```php
function tempnam(string $directory, string $prefix): string
```

Creates a file with a unique filename.

**Parameters**:
- `$directory` (`string`)
- `$prefix` (`string`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/tempnam.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/tempnam.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `tempnam` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/tempnam.md).
