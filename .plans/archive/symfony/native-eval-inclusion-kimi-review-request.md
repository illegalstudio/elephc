Review the same proposed specification as GLM, independently checking its claims
against the supplied source packet. You have text only, no filesystem or tools.
Do not claim execution or a complete repository audit.

Specification hash: e5b947cfa89453be8a9f9bf1b023ca0fa638e9feb2a6022119e4933f3e0254ef.

The packet contains the spec, GLM's final review, native/resolver/interface/include
source, and four failing CLI-backed tests. These integration tests use the real
compiled CLI and a non-test Magician archive; do not equate them with cfg(test)
stubs in the bridge. Also verify whether declared_once actually eliminates runtime
conditional includes before accepting GLM's assertion; insertion alone is not proof.

Judge specification readiness separately from implementation completion: RED tests
are expected before a TDD implementation. Identify missing architectural decisions,
reject unsafe shortcuts, and suggest precise state transitions and acceptance cases.
Do not assume every include failure undoes the include-once entry. Do not conflate
compiled signatures with executed declarations or treat all known files as loaded.

Answer in French, at most 1000 words, with facts versus hypotheses distinguished.
Finish with LOCK or NO LOCK for the exact hash and a short justification.
