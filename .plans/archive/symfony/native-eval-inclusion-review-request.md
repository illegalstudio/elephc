You are reviewing a proposed compiler/runtime correction, not implementing it.
Only the specification and source files concatenated after this request are
available to you. Do not claim filesystem access, executed tests, or php-src
verification beyond the supplied evidence.

Specification SHA-256: e5b947cfa89453be8a9f9bf1b023ca0fa638e9feb2a6022119e4933f3e0254ef

Identify concrete semantic gaps, unsafe shortcuts and missing acceptance tests.
Propose a precise implementation boundary preserving compiled execution, caller
scope, true include-once behavior and actual declaration activation. Distinguish
facts shown by code from hypotheses. In particular, is native/eval registry
synchronization sufficient? How must discovery differ from execution?

Do not approve ignoring existing declarations or marking every discovered file
as loaded. Answer in French, at most 1000 words. End with LOCK or NO LOCK for
the exact specification hash, explaining why. A draft with unresolved essential
architecture is not implementation-ready even if its high-level intent is sound.

Files follow in order: specification; native include lowering; resolver include
expansion; interface declaration execution; dynamic include execution; tests.
