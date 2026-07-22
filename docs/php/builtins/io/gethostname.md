---
title: "gethostname()"
description: "Gets the standard host name for the local machine."
sidebar:
  order: 185
---

## gethostname()

```php
function gethostname(): string
```

Gets the standard host name for the local machine.

**Parameters**: none.

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/network_env/gethostname.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/network_env/gethostname.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `gethostname` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/gethostname.md).
