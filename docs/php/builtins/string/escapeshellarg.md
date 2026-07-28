---
title: "escapeshellarg()"
description: "Quotes one argument for safe use in a shell command."
sidebar:
  order: 370
---

## escapeshellarg()

```php
function escapeshellarg(string $arg): string
```

Quotes one argument for safe use in a shell command.

**Parameters**:
- `$arg` (`string`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/escapeshellarg.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/escapeshellarg.rs)).

**Examples**:

See [`examples/shell-escaping`](../../../../examples/shell-escaping/main.php) for argument and command escaping together.

**Notes**:
- macOS and Linux use PHP's single-quoted shell form; Windows uses PHP's double-quoted command-line escaping rules.
- Embedded NUL bytes raise a catchable `ValueError`.
- Windows enforces PHP's 8192-byte command-line limit for both input and output.




## Internals

For how `escapeshellarg` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/escapeshellarg.md).
