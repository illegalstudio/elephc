---
title: "escapeshellcmd()"
description: "Escapes shell metacharacters in a command string."
sidebar:
  order: 371
---

## escapeshellcmd()

```php
function escapeshellcmd(string $command): string
```

Escapes shell metacharacters in a command string.

**Parameters**:
- `$command` (`string`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/escapeshellcmd.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/escapeshellcmd.rs)).

**Examples**:

See [`examples/shell-escaping`](../../../../examples/shell-escaping/main.php) for argument and command escaping together.

**Notes**:
- Shell metacharacters follow PHP's platform-specific POSIX or Windows rules.
- Embedded NUL bytes raise a catchable `ValueError`.
- Windows enforces PHP's 8192-byte command-line limit for both input and output.




## Internals

For how `escapeshellcmd` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/escapeshellcmd.md).
