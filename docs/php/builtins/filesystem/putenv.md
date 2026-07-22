---
title: "putenv()"
description: "Sets an environment variable."
sidebar:
  order: 142
---

## putenv()

```php
function putenv(string $assignment): bool
```

Sets an environment variable.

**Parameters**:
- `$assignment` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/network_env/putenv.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/network_env/putenv.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `putenv` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/putenv.md).
