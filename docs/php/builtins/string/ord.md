---
title: "ord()"
description: "Returns the ASCII value of the first character of a string."
sidebar:
  order: 378
---

## ord()

```php
function ord(string $character): int
```

Returns the ASCII value of the first character of a string.

**Parameters**:
- `$character` (`string`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/ord.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/ord.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `ord` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/ord.md).

