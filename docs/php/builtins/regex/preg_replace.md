---
title: "preg_replace()"
description: "Performs a regular expression search and replace."
sidebar:
  order: 342
---

## preg_replace()

```php
function preg_replace(string $pattern, string $replacement, string $subject): string
```

Performs a regular expression search and replace.

**Parameters**:
- `$pattern` (`string`)
- `$replacement` (`string`)
- `$subject` (`string`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/regex/preg_replace.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/regex/preg_replace.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `preg_replace` is implemented in the compiler, see [the internals page](../../../internals/builtins/regex/preg_replace.md).
