---
title: "fgets()"
description: "Gets line from file pointer."
sidebar:
  order: 166
---

## fgets()

```php
function fgets(resource $stream): mixed
```

Gets line from file pointer.

**Parameters**:
- `$stream` (`resource`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/fgets.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/fgets.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `fgets` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/fgets.md).
