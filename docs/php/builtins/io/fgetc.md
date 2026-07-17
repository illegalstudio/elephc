---
title: "fgetc()"
description: "Gets a character from the given file pointer."
sidebar:
  order: 162
---

## fgetc()

```php
function fgetc(resource $stream): mixed
```

Gets a character from the given file pointer.

**Parameters**:
- `$stream` (`resource`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/fgetc.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/fgetc.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `fgetc` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/fgetc.md).

