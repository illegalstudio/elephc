---
title: "stripslashes()"
description: "Removes backslashes from a string previously escaped by addslashes."
sidebar:
  order: 397
---

## stripslashes()

```php
function stripslashes(string $string): string
```

Removes backslashes from a string previously escaped by addslashes.

**Parameters**:
- `$string` (`string`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/stripslashes.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/stripslashes.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `stripslashes` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/stripslashes.md).

