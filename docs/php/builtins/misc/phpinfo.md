---
title: "phpinfo()"
description: "Outputs information about the PHP runtime and its loaded modules."
sidebar:
  order: 751
---

## phpinfo()

```php
function phpinfo(int $flags = 4294967295): bool
```

Outputs information about the PHP runtime and its loaded modules.

**Parameters**:
- `$flags` (`int`), default `4294967295`, optional

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/network_env/phpinfo.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/network_env/phpinfo.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `phpinfo` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/phpinfo.md).
