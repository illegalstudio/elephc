Independent intermediate review, not final acceptance. You have only the text
packet following this request. No filesystem access or test execution is implied.

Review spec hash e5b947cfa89453be8a9f9bf1b023ca0fa638e9feb2a6022119e4933f3e0254ef,
the two peer reviews, supplied code and newly measured PHP 8.5.6 CLI oracle.

Crucial oracle observations (php -n, files/output supplied below):
- A runtime exception after opening an include-once file leaves it registered;
  its unconditional class/interface/function declarations are already visible.
- A ParseError also leaves the file registered and the second include_once
  returns true; the class in that syntactically invalid file is not declared.
- A missing file remains unregistered and each include_once retries/fails.
Distinguish file registration, successful compilation/early binding, declaration
activation and body execution. An Entered/Failed design must not erase these facts.

Check peer assertions rather than adopting them: the supplied resolver inserts
declared_once, but no declared_once.contains query was found in resolver sources;
the set is exported as a sorted OPcache manifest input. Insertion alone does not
prove elimination of a later conditional runtime include. The CLI-backed tests
run real production-mode staticlibs, not the cfg(test) bridge stub.

Propose concrete decisions needed for an implementable spec: one request-local
registration authority, typed source identity, compiled file entry/caller scope,
and active symbols distinct from compiled metadata. Reject blanket no-ops,
eagerly marking discovered files loaded, and framework-specific exceptions.
Classify facts, hypotheses and missing evidence. RED tests are appropriate before
TDD implementation and do not themselves prevent approving a precise spec.
Answer in French, at most 1000 words; end with LOCK or NO LOCK for the exact hash.

Packet order: spec, GLM final review, Kimi 2.7 review, PHP oracle main/throws/invalid
and output, resolver entry/include code, native guards, dynamic include code,
and cross-engine regression tests.
