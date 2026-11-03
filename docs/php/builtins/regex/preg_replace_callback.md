---
title: "preg_replace_callback()"
description: "Performs a regular expression search and replace using a callback."
sidebar:
  order: 875
---

## preg_replace_callback()

```php
function preg_replace_callback(string $pattern, callable $callback, string $subject, int $limit = -1, mixed $count = null, int $flags = 0): mixed
```

Performs a regular expression search and replace using a callback.

**Parameters**:
- `$pattern` (`string`)
- `$callback` (`callable`)
- `$subject` (`string`)
- `$limit` (`int`), default `-1`, optional
- `$count` (`mixed`), passed by reference, default `null`, optional
- `$flags` (`int`), default `0`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/regex/preg_replace_callback.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/regex/preg_replace_callback.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `preg_replace_callback` is implemented in the compiler, see [the internals page](../../../internals/builtins/regex/preg_replace_callback.md).
