---
title: "file()"
description: "Reads an entire file into an array."
sidebar:
  order: 175
---

## file()

```php
function file(string $filename, int $flags = 0): mixed
```

Reads an entire file into an array.

**Parameters**:
- `$filename` (`string`)
- `$flags` (`int`), default `0`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/file.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/file.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `file` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/file.md).
