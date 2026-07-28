---
title: "readline()"
description: "Reads a line from the user's terminal."
sidebar:
  order: 334
---

## readline()

```php
function readline(string $prompt = null): mixed
```

Reads a line from the user's terminal.

**Parameters**:
- `$prompt` (`string`), default `null`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/readline.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/readline.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `readline` is implemented in the compiler, see [the internals page](../../../internals/builtins/process/readline.md).
