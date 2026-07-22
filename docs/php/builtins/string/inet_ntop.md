---
title: "inet_ntop()"
description: "Converts a packed internet address to a human-readable representation."
sidebar:
  order: 384
---

## inet_ntop()

```php
function inet_ntop(string $ip): mixed
```

Converts a packed internet address to a human-readable representation.

**Parameters**:
- `$ip` (`string`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/network_env/inet_ntop.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/network_env/inet_ntop.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `inet_ntop` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/inet_ntop.md).
