---
title: "file_get_contents()"
description: "Reads an entire file into a string."
sidebar:
  order: 166
---

## file_get_contents()

```php
function file_get_contents(string $filename): mixed
```

Reads an entire file into a string.

**Parameters**:
- `$filename` (`string`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/file_get_contents.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/file_get_contents.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `file_get_contents` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/file_get_contents.md).

