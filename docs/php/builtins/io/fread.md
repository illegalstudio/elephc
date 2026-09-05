---
title: "fread()"
description: "Binary-safe file read."
sidebar:
  order: 182
---

## fread()

```php
function fread(resource $stream, int $length): string
```

Binary-safe file read.

**Parameters**:
- `$stream` (`resource`)
- `$length` (`int`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/fread.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/fread.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `fread` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/fread.md).
