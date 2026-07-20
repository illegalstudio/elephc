---
title: "inet_pton()"
description: "Converts a human-readable IP address to its packed in_addr representation."
sidebar:
  order: 383
---

## inet_pton()

```php
function inet_pton(string $ip): mixed
```

Converts a human-readable IP address to its packed in_addr representation.

**Parameters**:
- `$ip` (`string`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/network_env/inet_pton.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/network_env/inet_pton.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `inet_pton` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/inet_pton.md).

