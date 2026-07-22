---
title: "getcwd()"
description: "Gets the current working directory."
sidebar:
  order: 125
---

## getcwd()

```php
function getcwd(): string
```

Gets the current working directory.

**Parameters**: none.

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/getcwd.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/getcwd.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `getcwd` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/getcwd.md).
