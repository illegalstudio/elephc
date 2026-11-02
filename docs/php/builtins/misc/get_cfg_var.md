---
title: "get_cfg_var()"
description: "Returns a php.ini option's value, or false when it is not set."
sidebar:
  order: 702
---

## get_cfg_var()

```php
function get_cfg_var(string $option): mixed
```

Returns a php.ini option's value, or false when it is not set.

**Parameters**:
- `$option` (`string`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/network_env/get_cfg_var.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/network_env/get_cfg_var.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `get_cfg_var` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/get_cfg_var.md).
