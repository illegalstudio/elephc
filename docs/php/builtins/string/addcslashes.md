---
title: "addcslashes()"
description: "Returns a string with backslashes before characters listed in a character set, including ranges."
sidebar:
  order: 859
---

## addcslashes()

```php
function addcslashes(string $string, string $characters): string
```

Returns a string with backslashes before characters listed in a character set, including ranges.

**Parameters**:
- `$string` (`string`)
- `$characters` (`string`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/addcslashes.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/addcslashes.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `addcslashes` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/addcslashes.md).
