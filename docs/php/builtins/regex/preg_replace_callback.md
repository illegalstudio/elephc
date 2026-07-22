---
title: "preg_replace_callback()"
description: "Performs a regular expression search and replace using a callback."
sidebar:
  order: 338
---

## preg_replace_callback()

```php
function preg_replace_callback(string $pattern, callable $callback, string $subject): string
```

Performs a regular expression search and replace using a callback.

**Parameters**:
- `$pattern` (`string`)
- `$callback` (`callable`)
- `$subject` (`string`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/regex/preg_replace_callback.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/regex/preg_replace_callback.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `preg_replace_callback` is implemented in the compiler, see [the internals page](../../../internals/builtins/regex/preg_replace_callback.md).
