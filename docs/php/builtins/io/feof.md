---
title: "feof()"
description: "Tests for end-of-file on a file pointer."
sidebar:
  order: 160
---

## feof()

```php
function feof(resource $stream): bool
```

Tests for end-of-file on a file pointer.

**Parameters**:
- `$stream` (`resource`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/feof.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/feof.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `feof` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/feof.md).

