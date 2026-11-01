---
id: gotcha-full-parallel-codegen-is-slower-than-the-deferri-e9438af3
type: gotcha
title: "Full-parallel codegen is slower than the deferring hybrid; the lever is emitted VOLUME"
description: "The zero-deferral configuration was reached and measured, and it lost; what to optimise instead, with the attribution method"
created: 2026-09-20
sources:
  - path: src/codegen/block_emit.rs
    blob: 069f11595b18a1e36f1823ee6486f4c55c35c800
  - path: src/codegen/shared_static_throw.rs
    blob: cb1d3b4ca52a1857bcd32f6582761b9b4d479784
supersedes: "gotcha-not-every-shared-codegen-cache-is-a-lookup-some--f74a1f68"
---

# Full-parallel codegen is slower than the deferring hybrid; the lever is emitted VOLUME

## The hypothesis, and the measurement that killed it

Making every shared-codegen cache reachable from a worker DOES reach zero deferral:

    deferred_share=0.0%  worker_pass=98.04s

against the deferring hybrid that is in the tree:

    worker_pass=38.23s + serial_tail=41.06s = 79.3 s

Full parallelism is SLOWER on the Symfony module. The remaining link failure in that
configuration (50 572 undefined `callable_invoker` symbols) was therefore not worth chasing.

## What to optimise instead

Codegen is 55% of the build and assembling 23%, and both scale with the SIZE of the emitted
assembly, so volume is the lever both phases feel. Attribution recipe, on the finished `.s`:

    awk 'function kindof(s,  t,n){sub(/:$/,"",s); sub(/_[0-9a-f]{12}_[0-9]+$/,"",s);
           sub(/_[0-9]+$/,"",s); n=split(s,t,"_"); return n>=3 ? t[n-2]"_"t[n-1]"_"t[n] : s}
         /^L?_[A-Za-z][A-Za-z0-9_]*:$/ { if (cur!="") {size[cur]+=n; cnt[cur]++}
                                         cur=kindof($0); n=0; next } { n++ }
         END { for (k in size) printf "%d\t%d\t%s\n", size[k], cnt[k], k }' index.s | sort -rn

It measures each label to the NEXT label, so a sparse family (`elephc_relaxed_branch`) is
over-counted; re-measure a top family against its real terminator before believing it.

Measured on Symfony `--web` (2 048 800 154 bytes, 61 455 536 lines) and fixed:

| idiom | sites | lines | share | fix |
|---|---|---|---|---|
| codegen-raised throwable | 165 115 | 8 834 931 | 14.4% | one shared helper + an interned record |
| `[$obj,'name']` static chain | 189 355 | 3 971 055 | 6.5% | the composite hash table its siblings already used |
| typed-property fatal | 56 280 | 2 363 760 | 3.8% | the same helper, report kind in the record |

Result: **1 528 866 488 bytes, 45 318 152 lines, binary 253 735 000 -> 193 279 496** — asm
-25.4%, lines -26.3%, binary -23.8%, all eight Symfony routes still byte-identical to `php -S`.

Still on the list, same attribution: `descriptor_callback_wrapper` 3.8%, `mixed_dyn_prop_set`
chain 3.5%, `eval_status_ok` 2.4%, the exception-BOUNDED callable invokers 1.9% (the only
invoker family with no cache at all), `ctor_mixed_case` 1.4%. The chains are all one defect
shape: a linear walk over every candidate class, which is also O(n) at RUN time.

## Why

Two sessions have now tried to make every codegen body worker-eligible. It is measurable and it does not pay.
