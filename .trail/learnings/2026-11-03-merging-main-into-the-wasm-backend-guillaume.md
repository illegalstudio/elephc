---
title: "Merging main into the wasm backend: what breaks and where to gate"
author: Guillaume
date: 2026-11-03
branch: feat/wasm-target
touches:
  - src/codegen_wasm/capability.rs
---

## Decided
- New native lowering from main is kept for native and gated with ctx.target.is_wasm() back to the branch's path when the WASM backend lacks the op or the ownership contract. The 2026-11 re-merge (309b0253a0) needed ~20 such gates; grep 'is_wasm' in src/ir_lower to see them.
- New Op / RuntimeFnId / RuntimeCallTarget variants are refused in capability.rs (op_is_supported, runtime_function_is_supported) and listed as Missing in inventory/classify.rs; regenerate Op::all and RuntimeFnId::all from the enum order, then gen_wasm_inventory, repin the guide hash.
- String call-argument pins are handed to the call on every target (a string pin is a copy the callee may hand back); only refcounted pins stay roots.

## Tried and ruled out
- Disabling the WASM audit for new shapes: the audit runs on unoptimized EIR, so optimizer passes cannot rescue a refused shape.

## Watch out
- Ownership contract drift is the dangerous class, not missing ops: mixed-slot PropSet adopts on WASM, WASM acquire of a string copies, method results are copied by the WASM Return epilogue. A main change that assumes retain-on-store or refcounted strings shows up as a UAF or leak only at run time (clone __clone hook, choose($x,false), Stack::clear).
- Main's numeric_storage replay widens loop counters to mixed; WASM skips it (its boxed slot already reads promoted floats correctly) or every $n % $i is refused.
- Preludes injected on reflection/dynamic calls (tokenizer, bcmath, WeakMap) carry externs that make the whole module refused on WASM; gate their heuristic triggers.
- Probes under elephc-scratch pick up its elephc.toml hosted extensions; wasmer lives at elephc-scratch/wasmer/dist/bin and runs ./main.wasm without --dir.
