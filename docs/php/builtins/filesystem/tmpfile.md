---
title: "tmpfile()"
description: "Creates a temporary file."
sidebar:
  order: 155
---

## tmpfile()

```php
function tmpfile(): mixed
```

Creates a temporary file.

**Parameters**: none.

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/tmpfile.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/tmpfile.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `tmpfile` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/tmpfile.md).
