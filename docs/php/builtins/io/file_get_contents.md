---
title: "file_get_contents()"
description: "Reads an entire file into a string."
sidebar:
  order: 176
---

## file_get_contents()

```php
function file_get_contents(string $filename, bool $use_include_path = false, mixed $context = null, int $offset = 0, int $length = null): mixed
```

Reads an entire file into a string.

**Parameters**:
- `$filename` (`string`)
- `$use_include_path` (`bool`), default `false`, optional
- `$context` (`mixed`), default `null`, optional
- `$offset` (`int`), default `0`, optional
- `$length` (`int`), default `null`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/file_get_contents.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/file_get_contents.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `file_get_contents` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/file_get_contents.md).
