---
title: "strlen()"
description: "Returns the length of a string."
sidebar:
  order: 420
---

## strlen()

```php
function strlen(string $string): int
```

Returns the length of a string.

**Parameters**:
- `$string` (`string`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/strlen.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/strlen.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `strlen` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/strlen.md).
