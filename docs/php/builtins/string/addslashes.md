---
title: "addslashes()"
description: "Adds backslashes before characters that need to be escaped."
sidebar:
  order: 343
---

## addslashes()

```php
function addslashes(string $string): string
```

Adds backslashes before characters that need to be escaped.

**Parameters**:
- `$string` (`string`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/addslashes.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/addslashes.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `addslashes` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/addslashes.md).

