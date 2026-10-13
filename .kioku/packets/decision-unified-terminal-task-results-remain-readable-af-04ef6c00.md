---
id: decision-unified-terminal-task-results-remain-readable-af-04ef6c00
type: decision
title: "Unified terminal task results remain readable after run scope closes"
description: "Maintainer selects completed-result access after run for both modes; task retains terminal parent-side outcome only, no live scope/worker; periodic authority still closes."
created: 2026-10-12
sources:
  - path: .plans/task-api-unification.md
    blob: 2fa5ed3dd0383287bac3af0035b02d2ac6d62db6
    lines: 35-52
    snip: 28a6c2ccecc8
    anchor: "One structured public group owns both execution domains. `spawn()` submits one"
---

# Unified terminal task results remain readable after run scope closes

## Fact

2026-10-12 maintainer explicitly chooses completed task results accessible after run returns. Unified Task result handle must retain terminal value/failure/cancellation without live group, worker or worker-arena/native-job ownership. Preserve repeated-await by-value semantics and parent-owned copied Parallel outcomes. This extends old Async closed-handle behavior and preserves Parallel post-scope results. TaskGroup and periodic control authority still end at scope close. Implementation and verification remain required.
