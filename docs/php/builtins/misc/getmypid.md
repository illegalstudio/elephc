---
title: "getmypid()"
description: "Gets the PHP process ID."
sidebar:
  order: 658
---

## getmypid()

```php
function getmypid(): int
```

Gets the PHP process ID.

**Parameters**: none.

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/network_env/getmypid.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/network_env/getmypid.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `getmypid` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/getmypid.md).
