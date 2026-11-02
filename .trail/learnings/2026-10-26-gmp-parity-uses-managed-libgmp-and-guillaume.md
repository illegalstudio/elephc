---
title: GMP parity uses managed libgmp and php-src 8.5 PHPT corpus
author: Guillaume
date: 2026-10-26
branch: feat/gmp-extension
cost: { minutes: 35 }
touches:
  - ".plans/gmp-extension.md"
---

Decided: implement ext/gmp as the real libgmp through a pinned managed-native package plus an elephc-gmp staticlib bridge, not as a Rust bigint. php-src is itself a thin adapter over libgmp, and 8.5's 99-test corpus pins division modes, infinite two's-complement bitwise results, gmp_export layouts, mpz_get_si wraparound and the exact MT random stream; the 8192-bit cryptography fixtures also rule out a slow pure-Rust modpow. The prelude class stores a decimal string, exposes num only through __debugInfo, and dispatches every operator to a static adapter so PHP 8.5 weak/strict float, null, array, resource and foreign-object diagnostics stay in one place. Engine gaps found: no internal-class int/float/bool cast contract (BcMath Number (int) currently dies with 'int cast for PHP type Object'), no bitwise/shift/unary object operators, and GMP's legacy C: unserialize is a separate path from Serializable. Worktree: .claude/worktrees/gmp-extension, branch feat/gmp-extension from origin/main. Beads: elephc-dci.4.1..7.
