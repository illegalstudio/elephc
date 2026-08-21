# PHP DOM 8.5 compliance specification

Status: review candidate, revision 6. Implementation is forbidden until GLM
5.2, Kimi K2.7, and Kimi K3 lock the same byte-for-byte revision.

## 1. Objective and definition of complete

Elephc shall provide the DOM extension exposed by PHP 8.5.8 on every supported
Elephc target. A compiled program that enables DOM shall need no separately
installed libxml2, Lexbor, PHP runtime, or DOM package on the destination
machine.

In this specification, “complete”, “100%”, and “PHP compliant” mean all of the
following at once:

1. The PHP-visible surface matches PHP 8.5.8: names, namespaces, aliases,
   casing, inheritance, interfaces, enum cases, final/abstract/internal status,
   methods, visibility, staticness, parameter names and order, named arguments,
   defaults, variadics, by-reference markers, return types, tentative return
   types, properties, readonly/virtual behaviour, constants, functions,
   deprecations, and Reflection output.
2. Observable behaviour matches the pinned PHP oracle for successful results,
   parsing, tree mutation, serialization, node and wrapper identity, liveness,
   iteration, namespaces, encodings, streams, callbacks, warnings, exceptions,
   fatals, and post-failure state.
3. The implementation preserves PHP object and native lifetime rules. Creating
   an equivalent wrapper is not acceptable where PHP returns the same wrapper
   object.
4. Every upstream DOM PHPT is represented in a checked-in ledger. Completion
   permits no skip, expected failure, or exclusion caused by missing DOM
   behaviour, diagnostics, Reflection, a companion dependency, or one of the
   supported targets.
5. The dependency closure required by DOM is real rather than mocked:
   the PHP 8.5.8 libxml surface is complete, and the PHP 8.5.8 SimpleXML
   extension is complete because DOM exposes bidirectional SimpleXML import.
6. `macos-aarch64`, `linux-aarch64`, and `linux-x86_64` have identical public
   semantics. Target-only placeholders, missing runtime symbols, system-library
   fallbacks, and reduced-validation builds are incomplete.
7. Examples, public and internals documentation, focused compiler/bridge tests,
   differential tests, upstream ledgers, error tests, ownership/GC tests, and
   supported-target CI ship with the feature.

Passing a parser or type-checker test, declaring the classes, or parsing a
representative document is progress, not completion.

## 2. Frozen reference and provenance

### 2.1 PHP source

The reference is the official PHP tag `php-8.5.8`, peeled Git commit
`26b97507444c4fbda072f57dda1820f7b7d5e467`.

The independent source archive is `php-8.5.8.tar.xz`, published 2 July 2026,
with SHA-256:

`58910198d19e873048fe87cdfe16bc790025417ede3d1651bfa1c4b533d573f2`

The archive URL is
`https://www.php.net/distributions/php-8.5.8.tar.xz`; the corresponding release
metadata is available from
`https://www.php.net/releases/index.php?json&version=8.5.8`.

Pinned Git objects and content digests are:

| Artifact | Git object or deterministic SHA-256 |
|---|---|
| `ext/dom` tree | `912205e4548629081e1f074eddb2df2899109177` |
| `ext/dom/tests` tree | `5ffbfb1cfe7418165176526f0a86568af58fd71a` |
| `ext/dom/php_dom.stub.php` blob | `7236e80b4d2475c349fd8dc43f0dbc5302fec035` |
| DOM stub SHA-256 | `bf8f0ca3fa9e322367bc37f3335865364d36cd7f0af055f8162b530f1d28c956` |
| sorted DOM PHPT digest | `b8ad9a9366ddafa2442ae786e76bb653c0e9b8c840c8b0abab3dcc320b31d5f3` |
| `ext/libxml` tree | `dc060c967760d2244f427da5c461bf38b7f759ad` |
| libxml stub blob | `ce257b54ab4d5dae1f89637186e2861aa6c3d8e4` |
| libxml stub SHA-256 | `50f036a2e7420c96507d97a5143f7ff23573553cc797bc15d9709b54318ff639` |
| sorted libxml PHPT digest | `8ebbfea4d882c7f78e5541fce049910482e63f19000642734b51e7ad40084e66` |
| `ext/simplexml` tree | `fefa5563ff5b5336cc468f77a55bb7e725ff9011` |
| SimpleXML stub blob | `935af16621e9d68a81c5b54c6927ecd93ca5802d` |
| SimpleXML stub SHA-256 | `1c8c2d6518a074c199c276bdd241e9f25152ca29160ffeadf66bfbc684d840a9` |
| sorted SimpleXML PHPT digest | `6f43e190d3931b41b627f0d5d09ad88a46b2ac35ab82e11d0db0df8f1b0e6d68` |
| bundled `ext/lexbor` tree | `6bdcf7d6e7e9bd3946e87dda140ab1f8e4ef47be` |
| PHP-adapted `ext/dom/lexbor` tree | `5b95c87cd4cbec6cb1eac347e79471fad79691b0` |

The bundled Lexbor version at that tag is 2.7.0.

### 2.2 Native engine configuration

The XML engine is libxml2 2.15.3 from the official GNOME archive
`libxml2-2.15.3.tar.xz`, SHA-256:

`78262a6e7ac170d6528ebfe2efccdf220191a5af6a6cd61ea4a9a9a5042c7a07`

The archive URL is
`https://download.gnome.org/sources/libxml2/2.15/libxml2-2.15.3.tar.xz`;
the publisher's checksum file is in the same directory.

It is built statically with threads, tree, output, push/reader, patterns,
writer, SAXv1, DTD validation, HTML, C14N, catalogs, XPath, XPointer, XInclude,
iconv, ISO-8859-X, regexps, automata, Relax NG, and XML Schema support.
Dynamic modules, Python bindings, and zlib are disabled. libxml2's removed HTTP
loader is not restored; PHP stream callbacks own external resource loading.

Modern HTML parsing and selectors use the exact Lexbor 2.7.0 and PHP adapter
sources named above. They are compiled into the bridge. No destination-system
Lexbor is used.

Vendored archives/sources retain upstream licenses and notices. Builds are
offline and checksum-verified. Changing an engine version, build feature, patch,
or source digest is a reference change that invalidates every review lock.

### 2.3 Differential oracle

The executable oracle is PHP 8.5.8 built from the official archive with:

- CLI, DOM, libxml, SimpleXML, and XML enabled;
- the pinned static libxml2 2.15.3;
- PHP's bundled Lexbor 2.7.0;
- `LIBXML_DOTTED_VERSION=2.15.3` and `LIBXML_VERSION=21503`.

The oracle build recipe and its configure/build logs are checked in as
reproducible test metadata. It is built in CI rather than committed as a binary.

### 2.4 Normative precedence

The following rules resolve conflicts:

1. PHP-visible declarations are determined by the pinned stubs and exact
   Reflection snapshot from the oracle.
2. Observable results and diagnostics are determined by the matching upstream
   PHPT, then the implementation at the pinned php-src commit, then an isolated
   oracle probe.
3. DOM algorithms are determined by php-src even when a web standard or native
   dependency behaves differently.
4. `ext/libxml` and `ext/simplexml` use the same ordering for their companion
   surfaces.
5. The English PHP manual explains intended public behaviour; the French manual
   is a documentation cross-check. Neither overrides source, PHPT, or the oracle.
6. A platform-conditional PHPT selects the branch applicable to the target. A
   disagreement is never hidden by an undocumented output normalization.

## 3. Frozen public surface

### 3.1 DOM surface accounting

`ReflectionExtension("dom")->getClasses()` exports 51 names. They resolve to 50
canonical type definitions because the case-insensitive exported alias
`Dom\DomException` resolves to canonical `DOMException`.

Across canonical definitions there are:

- 313 directly declared methods;
- 184 directly declared properties;
- 16 directly declared class constants or enum cases;
- 61 extension constants;
- two extension functions.

Naively iterating the 51 exported names counts 185 directly declared properties
because `DOMException::$code` is encountered through both names. The generated
parity test records both the exported-name view and canonical de-duplicated view.

### 3.2 Legacy global namespace

Classes:

- `DOMAttr`
- `DOMCdataSection`
- `DOMCharacterData`
- `DOMComment`
- `DOMDocument`
- `DOMDocumentFragment`
- `DOMDocumentType`
- `DOMElement`
- `DOMEntity`
- `DOMEntityReference`
- `DOMException`
- `DOMImplementation`
- `DOMNameSpaceNode`
- `DOMNamedNodeMap`
- `DOMNode`
- `DOMNodeList`
- `DOMNotation`
- `DOMProcessingInstruction`
- `DOMText`
- `DOMXPath`

Interfaces:

- `DOMChildNode`
- `DOMParentNode`

Function:

- `dom_import_simplexml(object $node): DOMAttr|DOMElement`

The pinned signature has exactly one parameter. It has no optional
`$class_name`; DOM wrapper subclass selection is controlled by
`registerNodeClass()`.

### 3.3 Modern `Dom` namespace

Classes:

- `Dom\Attr`
- `Dom\CDATASection`
- `Dom\CharacterData`
- `Dom\Comment`
- abstract `Dom\Document`
- `Dom\DocumentFragment`
- `Dom\DocumentType`
- `Dom\DtdNamedNodeMap`
- `Dom\Element`
- `Dom\Entity`
- `Dom\EntityReference`
- `Dom\HTMLCollection`
- final `Dom\HTMLDocument`
- `Dom\HTMLElement`
- `Dom\Implementation`
- `Dom\NamedNodeMap`
- readonly final `Dom\NamespaceInfo`
- `Dom\Node`
- `Dom\NodeList`
- `Dom\Notation`
- `Dom\ProcessingInstruction`
- `Dom\Text`
- final `Dom\TokenList`
- final `Dom\XMLDocument`
- final `Dom\XPath`

Alias:

- `Dom\DomException` is a case-insensitive alias of `DOMException`; Reflection
  reports canonical name `DOMException`. It is not a 51st definition.

Interfaces and enum:

- `Dom\ChildNode`
- `Dom\ParentNode`
- backed enum `Dom\AdjacentPosition`

`Dom\AdjacentPosition` has the exact backed cases:

- `BeforeBegin = "beforebegin"`
- `AfterBegin = "afterbegin"`
- `BeforeEnd = "beforeend"`
- `AfterEnd = "afterend"`

Function:

- `Dom\import_simplexml(object $node): Dom\Attr|Dom\Element`

This pinned signature likewise has exactly one parameter and no
`$class_name`.

The generated DOM manifest, not the explanatory lists above, freezes every
member, modifier, declaration order, default, alias, deprecation, and Reflection
field from `php_dom.stub.php` and the oracle snapshot.

The modifiers shown in the list are exhaustive for modern concrete classes:
`Dom\Document` is abstract; `Dom\HTMLDocument`, `Dom\XMLDocument`,
`Dom\TokenList`, `Dom\NamespaceInfo`, and `Dom\XPath` are final;
`Dom\NamespaceInfo` is readonly. The other listed modern classes are neither
abstract nor final at the pinned tag.

### 3.4 DOM and libxml constants

The legacy `XML_*_NODE`, `XML_ATTRIBUTE_*`, and `DOM_*_ERR` constants, the
modern namespaced `Dom\*_ERR` constants, `Dom\HTML_NO_DEFAULT_NS`, and the six
`DOCUMENT_POSITION_*` class constants have PHP's exact values and availability.

The complete pinned libxml constant surface is exposed, including version,
parser, serializer, error-level, schema, and legacy HTML flags. For this build:

- `LIBXML_VERSION` is `21503`;
- `LIBXML_DOTTED_VERSION` is `"2.15.3"`;
- `LIBXML_LOADED_VERSION` is the oracle's exact string;
- `LIBXML_NO_XXE` is present because `LIBXML_VERSION=21503` satisfies the
  pinned stub guard `LIBXML_VERSION >= 21300`; its value is
  libxml2's `XML_PARSE_NO_XXE`.

Accepted/rejected option masks are per-entry-point rules copied from php-src.
Modern APIs do not accept a flag merely because libxml2 defines it.

### 3.5 Complete libxml companion extension

The following complete PHP 8.5.8 surface is in scope:

- `LibXMLError` with its six typed public properties and exact construction,
  cloning, debug, and serialization restrictions;
- `libxml_set_streams_context()`;
- `libxml_use_internal_errors()`;
- `libxml_get_last_error()`;
- `libxml_get_errors()`;
- `libxml_clear_errors()`;
- deprecated `libxml_disable_entity_loader()`;
- `libxml_set_external_entity_loader()`;
- `libxml_get_external_entity_loader()`;
- every constant in the pinned libxml stub.

The six `LibXMLError` properties are ordinary writable typed public properties
on the pinned oracle; they are not specified as readonly.

The libxml error list is ordered, clearable, execution-context local, and shared
by DOM and SimpleXML exactly as in PHP. Warnings may coexist with a successful
return and are not collapsed into a single bridge status.

### 3.6 Complete SimpleXML dependency closure

The DOM import functions cannot be complete if Elephc can only manufacture a
mock `SimpleXMLElement`. This branch therefore implements the complete pinned
SimpleXML extension, not a DOM-only façade:

- `simplexml_load_file()`;
- `simplexml_load_string()`;
- `simplexml_import_dom()`;
- `SimpleXMLElement` with every method, magic object handler, interface,
  tentative return type, alias, and serialization restriction in the stub;
- `SimpleXMLIterator`.

The SimpleXML object handlers for property/attribute access, assignment,
iteration, casts, count, comparison, debug output, namespaces, XPath, mutation,
and subclass selection match php-src and its 156 PHPTs.

DOM and SimpleXML share the same libxml document/node state. Mutations are live
in both directions. Importing the same SimpleXML node repeatedly into one DOM
family returns the same wrapper. A node can be imported into the legacy or
modern family, never both:

- legacy first, then modern:
  `TypeError: Dom\import_simplexml(): Argument #1 ($node) must not be already imported as a DOMNode`
- modern first, then legacy:
  `TypeError: dom_import_simplexml(): Argument #1 ($node) must not be already imported as a Dom\Node`

These are oracle outputs and the expectations of
`ext/dom/tests/dom_import_simplexml.phpt` and
`ext/dom/tests/modern/xml/simplexml_interop.phpt`; the class is `TypeError`, not
`ValueError`.

## 4. PHP-visible semantic contract

### 4.1 Internal extension objects, not userland shims

DOM, libxml, and SimpleXML classes/functions are registered as compiler internal
extensions. They are not injected PHP source. Reflection reports internal
classes and functions with the exact PHP signatures and modifiers.

Virtual DOM properties use internal property-handler metadata matching php-src.
They are Reflection-visible as virtual properties, but they are not
user-visible PHP property hooks: on the pinned oracle
`ReflectionProperty::isVirtual()` is true and
`ReflectionProperty::getHooks()` is empty for both
`DOMNode::$nodeName` and `Dom\Node::$nodeName`. Compiler-only handles,
factories, finalizers, operation functions, callback shims, and buffers are
absent from:

- Reflection and generated PHP documentation;
- `get_object_vars()`, casts, `var_dump()`, and `__debugInfo()`;
- serialization;
- class, method, property, function, and constant catalogs;
- `class_exists()`, `method_exists()`, `property_exists()`, and
  `function_exists()`.

All direct calls, first-class callables, named/spread arguments, callbacks, and
runtime callables consume Elephc's shared call-argument planner. The DOM lowering
does not reconstruct PHP argument rules locally. Source evaluation order is
preserved before bridge/ABI order.

### 4.2 Document, node, and wrapper identity

The authoritative native tree is libxml2's `xmlDoc`/`xmlNode`/`xmlAttr` graph.
Each document, node, collection, XPath context, and SimpleXML view receives a
generation-checked bridge handle; raw pointers never enter PHP values.

Each execution context owns family-partitioned weak wrapper caches whose primary
key is native node identity within the execution-context/family partition. The
effective registered class chosen when a wrapper is first created is entry
metadata, not part of the lookup key. A later `registerNodeClass()` policy
change therefore affects only nodes without a live cached wrapper and never
creates a second wrapper for an already wrapped native node. A document
contributes its registration policy, but does not own the cache itself. This
also gives stable wrapper identity to documentless nodes such as a doctype
created by `DOMImplementation::createDocumentType()`. If such a node is later
attached, its existing wrapper remains authoritative. Repeated access through
navigation properties, `item()`, offsets, attributes, ID lookup, XPath,
selectors, collections, and SimpleXML import returns the same live PHP wrapper
when PHP does.

Detaching or reattaching a node does not invalidate its wrapper. A detached
node keeps its document allocation alive. Collections and XPath contexts retain
their documents. Wrapper destruction releases its handle and weak-cache entry
exactly once. Native document destruction occurs only when no owning handle can
reach it. PHP object cycles involving dynamic properties remain visible to and
collectable by Elephc's GC.

`===`, `isSameNode()`, `isEqualNode()`, and
`compareDocumentPosition()` preserve their distinct PHP meanings.
Disconnected-node ordering and bitmasks follow php-src.

### 4.3 Clone, import, adoption, and dynamic properties

Object `clone`, `cloneNode()`, `importNode()`, and `adoptNode()` are separate
operations with PHP's exact constructor, wrapper, owner-document, namespace,
ID-state, and descendant rules.

The pinned oracle establishes the dynamic-property rules for both families:

- creating a dynamic property emits PHP 8.5's deprecation;
- object `clone` creates a different wrapper and copies its PHP property bag;
- `cloneNode()` creates a different wrapper and does not copy dynamic
  properties;
- `adoptNode()` returns the same wrapper and preserves its dynamic properties;
- `importNode()` creates a different wrapper and does not copy dynamic
  properties.

Tree mutation never replaces a live wrapper merely to simplify caching.

### 4.4 Construction, registration, cloning, and serialization restrictions

Every class follows the stub/source constructor policy: public construction,
required initialization, engine-created-only construction, constructor
re-entry, subclass construction, finality, and uninitialized access all match
PHP.

`DOMDocument::registerNodeClass()` and
`Dom\Document::registerNodeClass()` enforce the precise allowed base/subclass
relations, null reset semantics, return values, family isolation, constructor
bypass, and diagnostics. The mapping affects future wrappers without replacing
an already cached wrapper.

Cloneable and non-cloneable objects, serializable and non-serializable objects,
unserialization, readonly/virtual property writes and unsets, collection
writes, and unsupported instantiation match PHP exactly.

The surface/semantic generator also emits a construction-state matrix for every
type: direct constructor availability and arity, engine-only factory, valid
subclass path, initialized native kinds, readable/writable properties before
initialization, and exact diagnostic on invalid access. Tests consume this
matrix; “uninitialized access matches PHP” is not implemented as one shared
fallback message.

### 4.5 Tree mutation and evaluation order

Hierarchy validation happens at PHP's observable point and before mutation
where PHP is atomic. Cycles, illegal document children, multiple document
elements, misplaced doctypes, attributes as children, foreign-document nodes,
absent reference children, and in-use attributes raise the exact PHP exception,
DOM code, message, and post-failure state.

`appendChild`, `insertBefore`, `replaceChild`, `removeChild`, ParentNode,
ChildNode, fragments, adjacent insertion, and `replaceChildren` preserve
self-insertion and evaluation rules. Node-or-string variadics are evaluated
exactly once in source order. Strings become text nodes in the target document.
Fragments splice their children and become empty.

Mutating methods invalidate only the affected native indexes and cached live
query observations. Their returned wrapper is selected through the common
identity factory.

The generated semantic manifest contains an outcome row for each mutator and
self/reference relation (`receiver`, `parent`, `new child`, `old child`,
`reference child`, ancestor, descendant, fragment, and document). Each row
records return/throw, DOM code/message, resulting order, and identity. Its
sources include the pinned `DOMElement_append_hierarchy_test.phpt`,
`DOMElement_prepend_hierarchy_test.phpt`,
`DOMCharacterData_replaceWith_self.phpt`, `bug81642.phpt`,
`DOMElement_replaceChildren.phpt`, and
`modern/spec/Node_replaceChild_edge_cases.phpt`. Therefore “self-insertion”
does not collapse distinct `appendChild`, `insertBefore`, `replaceChild`,
ParentNode, and ChildNode behaviours into one generic rule.

`normalize()` follows the exact php-src node-type cases: adjacent text merging,
empty text removal, CDATA boundaries, entity nodes, attributes, descendants,
and cache/ID updates are decided by the pinned source and PHPTs rather than a
generic normalization heuristic.

### 4.6 Namespaces, names, and attributes

Qualified-name validation, prefix/local-name splitting, case handling,
reserved `xml`/`xmlns` rules, null versus empty namespace distinctions,
namespace lookup, default namespaces, reconciliation on import/adopt and
serialization, prefix writes, and namespace declaration nodes match php-src.

Attributes are stable first-class native nodes with owner element, namespace,
prefix, ID state, and in-use checks. Attribute iteration order, missing
attribute results, namespace declaration exposure, and legacy/modern
differences match their PHPTs.

`className`, `id`, `classList`, ASCII-whitespace tokenization, duplicate-token
handling, `add`, `remove`, `toggle`, `replace`, `supports`, and token validation
use php-src's modern DOM algorithms and exception mapping. In particular,
`supports()` reproduces PHP's attribute-specific supported-token behaviour even
where a newer WHATWG table would differ; web-platform conformance never
overrides the pinned PHP implementation.

### 4.7 Character data and encodings

Character-data offsets and lengths use libxml2's UTF-8 code-point operations,
matching php-src: they count Unicode scalar code points, not bytes, grapheme
clusters, or UTF-16 code units. Integer overflow, negative values, and
out-of-range values retain the legacy/modern return-versus-throw distinction.

NUL bytes, invalid UTF-8, invalid XML characters, unpaired encoded input, and
encoding conversion failures follow the exact API-specific PHP result. The
implementation does not silently normalize Unicode.

XML declarations, BOMs, explicit encodings, document encoding properties, HTML
meta charset sniffing, transport hints supplied by streams, and serialization
encoding follow the PHP 8.5.8 + libxml2 2.15.3 + Lexbor 2.7.0 oracle. All
encodings available in that pinned build are included.

### 4.8 Collections, iteration, and mutation epochs

`DOMNodeList`, `DOMNamedNodeMap`, `Dom\NodeList`, `Dom\NamedNodeMap`,
`Dom\DtdNamedNodeMap`, `Dom\HTMLCollection`, and `Dom\TokenList` match PHP's
implemented interfaces, indexed/named access, iteration keys, count, read-only
behaviour, clone/serialization restrictions, and debug shape.

Each native document has a monotonically increasing mutation epoch. A live
collection stores its php-src-equivalent query descriptor, last observed epoch,
cached length, and optional cursor acceleration. On epoch change it recomputes
membership/order from the authoritative tree; the collection wrapper itself
remains identical. Mutation during iteration observes PHP's per-step behaviour.

Snapshot APIs retain an ordered handle vector. `querySelectorAll()` and XPath
node-set results do not become live merely because they share wrapper objects.
`length`, `count()`, `item()`, offsets, named lookup, and iteration agree at the
same observation point.

### 4.9 XML parsing and configuration

Legacy loaders and modern XML factories support strings, files, PHP streams,
base/effective URI, `documentURI`, `baseURI`, BOMs, declarations, whitespace,
entity substitution, external subsets, validation-on-parse, recovery, and exact
entry-point option allowlists.

Parser depth, name, text, entity, amplification, and allocation limits are the
limits of the pinned libxml2 configuration used by PHP. Elephc adds no smaller
DOM-specific cap. `LIBXML_PARSEHUGE` and `LIBXML_NO_XXE` modify the exact
libxml2 2.15.3 limits and entity policy. Integer/pointer overflow remains a
checked fatal/error path rather than undefined behaviour.

### 4.10 HTML parsing and serialization

Legacy `DOMDocument` HTML APIs use libxml2's HTML parser as PHP does.

`Dom\HTMLDocument` uses PHP's bundled Lexbor WHATWG parser and the exact
Lexbor-to-libxml2 bridge algorithms from php-src. This includes encoding
prescan, parse errors, templates, foreign SVG/MathML content, quirks modes,
foster parenting, raw-text elements, implied nodes, fragment context, and
`Dom\HTML_NO_DEFAULT_NS`.

Modern HTML serialization follows `html5_serializer.c`, including raw-text
escaping, void elements, optional syntax, namespace output, comments, doctypes,
fragments, and invalid tree cases. It does not delegate to libxml2's legacy HTML
serializer.

### 4.11 Streams, URIs, and external resources

All filename/URI accepting DOM, libxml, and SimpleXML operations enter through
Elephc's PHP stream layer. The host provides bytes, effective URI, stream
context, warning state, and write results to the bridge. Direct libxml file or
network opening is disabled.

libxml entity, DTD, schema, Relax NG, XInclude, and catalog callbacks route back
through the same host stream callback, including custom wrappers and the active
libxml stream context. Re-entrant loaders are supported. `LIBXML_NONET`,
`LIBXML_NO_XXE`, the external entity loader callable, and default-deny network
policy are enforced at the same stage as PHP.

A missing Elephc stream feature required by an upstream PHPT is implementation
work for this branch or must be represented by an equivalent fixture that
preserves every DOM observation. “Stream unsupported” is not a semantic skip.
Byte counts and partial/failed writes match PHP.

### 4.12 XML serialization

Legacy and modern XML output matches PHP for declarations, encodings, empty
elements, namespace fixup, escaping, CDATA, comments, processing instructions,
doctypes, fragments, subtrees, `formatOutput`, file byte counts, and invalid
tree failures.

Modern `innerHTML`, `outerHTML`, `substitutedNodeValue`, title/head/body
properties, and their mutations preserve XML-versus-HTML distinctions and
contextual fragment parsing.

### 4.13 Selectors and XPath

Selectors use PHP's adapted Lexbor selector engine over the authoritative
libxml2 tree. `querySelector`, `querySelectorAll`, `closest`, and `matches`
implement the exact grammar, namespace/scope rules, lists, escaping,
pseudo-classes, document order, invalid-selector exception, and snapshot
semantics tested upstream.

Legacy and modern XPath implement:

- context ownership and family checks;
- explicit namespace registration and automatic in-scope registration;
- `registerNodeNamespaces`;
- scalar conversions and ordered/deduplicated node sets;
- `query()` versus `evaluate()` return types;
- `quote()`;
- PHP callback registration restrictions and allowlists;
- namespaced callbacks and exact argument/result conversion;
- callback exception propagation, recursive XPath, and DOM re-entrancy.

Callback lookup and invocation use Elephc's runtime callable machinery, not a
bridge-local approximation. Returned nodes use the common wrapper cache.

### 4.14 Validation, XInclude, and canonicalization

DTD validation, XML Schema validation (including `LIBXML_SCHEMA_CREATE`), Relax
NG validation from file/source, XInclude, C14N, exclusive C14N, comments,
namespace-prefix filters, XPath filters, file output, default attributes,
return values, diagnostics, and tree side effects match PHP.

These operations use the pinned libxml2 implementation directly. Bridge
translation does not rebuild their algorithms. Any mutation updates epochs,
namespace state, and ID indexes before the next PHP observation.

### 4.15 Diagnostics and partial failure

Parity for every failure includes:

- thrown class (`TypeError`, `ValueError`, `DOMException`, `Error`, etc.);
- DOM exception code and exact message;
- warning, deprecation, and fatal category/text/order;
- return value;
- whether state was partially mutated;
- libxml error-buffer entries and their order;
- output/files produced before failure.

The bridge returns an ordered diagnostic vector separately from the primary
status, so warnings plus success are representable. The compiler maps structured
error identifiers to PHP text at the same layer as its other diagnostics.

The PHPT runner interprets upstream `EXPECT`, `EXPECTF`, and `EXPECTREGEX`
directly. There is no global output sanitizer. A translated fixture may replace
only fields already wildcarded by upstream or fields named individually in its
ledger entry, such as its isolated temporary-directory prefix. Object IDs,
paths, and line numbers are not silently discarded.

## 5. Implementation architecture

### 5.1 Bridge and reproducible native build

A workspace crate `crates/elephc-dom` produces `staticlib` and `rlib` outputs.
It owns the native engine build, safe handle layer, result protocol, and
libxml/Lexbor adapters.

The bridge table receives one declarative entry:

- library: `elephc_dom`;
- Cargo package: `elephc-dom`;
- Elephc force flag: `--with-dom`;
- library-directory override: `ELEPHC_DOM_LIB_DIR`;
- whole archive: false unless link testing proves registration needs it;
- target system links: derived declaratively, including macOS `iconv` where
  required and existing Linux C/pthread/dl requirements.

The operating system C/runtime libraries above are mandatory platform ABI
components, not semantic fallback engines. “No system-library fallback” forbids
substituting a destination libxml2/Lexbor or a reduced target implementation; it
does not forbid libc, pthread/dl, or macOS's platform iconv ABI.

`--with-dom` is Elephc's bridge-selection flag; it is not presented as PHP's
source configure option (`--enable-dom`). Direct/static DOM, libxml, and
SimpleXML references auto-enable the bridge. Literal Reflection,
`class_exists()`, `function_exists()`, `extension_loaded()`, callback, and
constant lookups participate in detection. Programs using a name known only at
runtime use `--with-dom`; the conformance suite always enables it.

The bridge statically contains the verified libxml2 and Lexbor objects. Build
scripts use the Cargo target C compiler and archiver, honor cross-compilation,
do not fetch the network, and fail on checksum/configuration drift. The produced
program has no destination package dependency.

### 5.2 Authoritative libxml graph and safe handles

The bridge does not copy XML into a second general-purpose Rust DOM. libxml2's
document/node/attribute graph is authoritative, matching php-src and avoiding
semantic drift in namespaces, XPath, validation, and serialization.

Opaque 64-bit handles index bridge-owned tables. A handle encodes table kind,
slot, and generation. Entries contain native pointers plus document/family
ownership metadata. Every operation validates kind, generation, context,
document, and legacy/modern family before dereferencing. Freed slots increment
their generation; stale, forged, cross-context, and cross-kind handles produce
structured internal errors without undefined behaviour.

Lexbor modern HTML parsing is converted into libxml2 nodes with PHP's pinned
`ext/dom/html5_parser.c` bridge logic. PHP's adapted selector layer operates on
the same libxml2 graph. No Lexbor DOM pointer escapes parsing/selector adapter
ownership.

### 5.3 Compiler internal-extension registry

The compiler gains a small general internal-extension registry rather than a
DOM-specific PHP prelude. Generated metadata describes:

- extensions and availability;
- classes, aliases, enums, interfaces, inheritance, modifiers, constants;
- methods/functions with the full shared call signature;
- native virtual properties and read/write/unset handlers;
- object-handler capabilities such as indexing, iteration, casts, compare,
  clone, debug, GC traversal, and serialization;
- an explicit operation opcode for each native action.

The registry feeds name resolution, type checking, Reflection metadata,
callable metadata, effects, EIR lowering, linking requirements, docs, and
runtime catalogs from the same generated source. Dynamic dispatch and
first-class callables retain the same signature metadata as direct calls.

DOM calls lower to a typed EIR `InternalExtensionCall` instruction (or an
equally general existing call instruction extended with internal-extension
metadata), followed by normal target-aware ABI lowering. No shared codegen file
contains raw ARM64/x86_64 register names or duplicate object-layout offsets.

The optimizer marks reads/mutations/callbacks/streams/diagnostics
conservatively. It never removes or reorders a DOM operation that can observe or
change tree state, wrapper identity, a PHP callback, a stream, diagnostics, or
dynamic properties.

### 5.4 PHP wrapper layout and weak cache

A native extension object is an ordinary GC-visible Elephc PHP object plus
compiler-hidden metadata:

- extension/type identifier;
- bridge execution-context handle;
- native object handle;
- family and registered-class identifier;
- finalization state.

Hidden fields are allocated through common object/frame/ABI helpers and excluded
from public property tables. Dynamic properties use the normal PHP object
property bag so GC sees their references.

The wrapper factory:

1. validates the native handle and requested family;
2. applies the document's `registerNodeClass()` mapping;
3. returns the live wrapper already registered for that native node, including
   when the document's class mapping has changed since wrapper creation;
4. otherwise allocates the exact internal/registered PHP class without invoking
   a user constructor where PHP bypasses it;
5. inserts a weak cache entry only after successful initialization;
6. rolls back both sides on allocation failure.

Finalization removes the cache entry only if it still names that wrapper and
releases the native handle exactly once. Clone/import/adopt use explicit factory
modes implementing section 4.3.

### 5.5 Execution and concurrency model

One DOM bridge context exists per PHP execution context:

- a CLI program has one context;
- an Elephc web worker is a forked process with a current-thread runtime;
- a PHP handler runs synchronously to completion on that worker's one thread;
- concurrent requests execute in separate worker processes.

DOM handles are context-bound and never `Send` across PHP execution threads.
After `fork`, each worker initializes its own context before the first request;
the master creates none. Request reset clears libxml errors, callbacks, stream
context, temporary results, and non-persistent wrapper state at PHP's request
boundary. Process-local immutable engine tables may be reused.

The ABI remains re-entrant on the same thread because XPath and entity-loader
callbacks can call PHP, which can call DOM again. It therefore uses independent
result frames, not a single “last result” buffer.

### 5.6 Versioned C ABI

The public native boundary uses only `extern "C"`, fixed-width integers,
byte pointers plus lengths, and `#[repr(C)]` records. Rust references, `String`,
`Vec`, trait objects, enum layout, and panics never cross it.

The ABI exposes:

```text
elephc_dom_context_new(host_vtable, out_context) -> status
elephc_dom_context_reset(context) -> status
elephc_dom_context_free(context)
elephc_dom_call(context, request_ptr, request_len, out_result) -> status
elephc_dom_result_release(context, result_id)
```

The request is a versioned, bounds-checked flat message:

```text
RequestHeader {
    abi_version: u32,
    header_size: u32,
    opcode: u32,
    flags: u32,
    receiver: u64,
    value_count: u64,
    byte_count: u64
}
Value {
    tag: u32,
    flags: u32,
    payload0: u64,
    payload1: u64
}
```

`RequestHeader::opcode` is the sole opcode; it is not duplicated as a C
function argument.

Scalar payloads are inline. Strings use offset plus length into the request byte
section and therefore preserve embedded NUL. Arrays/maps use validated ranges
of subsequent `Value` entries with explicit key tags. Node/object/callable/
resource values are opaque host or bridge handles. Cycles are rejected where
the PHP signature cannot accept them and represented through host handles where
callbacks require them. All offset/count arithmetic is checked.

The bridge writes a fixed-size result header:

```text
Result {
    abi_version: u32,
    struct_size: u32,
    status: u32,
    value_tag: u32,
    php_error_kind: u32,
    dom_exception_code: i32,
    result_id: u64,
    payload0: u64,
    payload1: u64,
    bytes_ptr: *const u8,
    bytes_len: u64,
    values_ptr: *const Value,
    values_len: u64,
    diagnostics_ptr: *const Diagnostic,
    diagnostics_len: u64
}
```

The generated stable status table is `OK = 0`, `THROW = 1`, `FATAL = 2`,
`ABI_ERROR = 3`, `INTERNAL_PANIC = 4`, and `MALFORMED_REQUEST = 5`. The same
values apply to the C function return status and, whenever `out_result` is
writable, `Result::status`.
Diagnostics have independent warning/deprecation/libxml levels and source
records, so `OK` plus warnings is valid.

`dom_exception_code` is zero unless the primary `THROW` is a `DOMException`;
the compiler rejects an invalid status/error-kind/code combination as
`ABI_ERROR` rather than treating it as a successful DOM code.

Every call that writes `out_result`, regardless of status, owns a distinct
nonzero `result_id`. All of its byte, value, and diagnostic pointers remain
valid until `elephc_dom_result_release(context, result_id)` for that exact ID.
Nested/re-entrant calls cannot invalidate an outer successful or error result.
PHP copies byte/array/error results or adopts returned native handles before
release.

A double, unknown, or foreign-context release is a no-op in production: it
never frees another result, mutates the context, or causes undefined behaviour.
The bridge records a structured invalid-release event in a test/instrumentation
counter so adversarial tests can assert detection without adding a PHP-visible
diagnostic or a fallible production release ABI.

If the request contains a readable ABI prefix (`abi_version` and
`header_size`) but names an unsupported ABI version, the bridge returns
`ABI_ERROR` before decoding an opcode and performs no DOM mutation. A request
shorter than that prefix, or whose declared header is unreadable, returns
`MALFORMED_REQUEST`; no opcode is decoded, so no mutation path is entered. With
a writable `out_result`, these paths initialize its scalar header/status and
owned `result_id` but publish no byte, value, or diagnostic payload pointers.
The opcode table is generated from the locked surface manifest with explicit
stable numeric values and a manifest digest. An opcode/signature change bumps
the ABI version.

### 5.7 Host callback ABI

`context_new` receives a versioned host vtable with its struct size and one
generic re-entrant host-call entry point. Host opcodes cover:

- PHP stream read/write/stat and effective URI;
- external entity loader invocation;
- PHP callable invocation for XPath;
- runtime value retain/release;
- ordered warning/deprecation/fatal delivery when immediate delivery is required.

Host requests/results use the same value/message encoding and caller-owned
lifetimes. No callback retains a raw host pointer after returning unless it has
received an explicit retained handle. Callback exceptions return `THROW` and
unwind through result statuses, never across C/Rust frames.

### 5.8 Panic, OOM, and native error containment

Every exported Rust entry point and every host callback trampoline is protected
by `catch_unwind`. A panic becomes `INTERNAL_PANIC`; production text is stable
and hides Rust internals, while a test-only injection verifies cleanup.

No Rust panic, PHP exception unwind, or native non-local exit crosses the ABI.
libxml structured errors are captured into the current context/result and
translated after native calls return.

Recoverable libxml/Lexbor allocation failures become PHP's corresponding
warning/error/fatal path without dereferencing null. Rust global allocator
exhaustion follows Elephc's existing process-fatal policy. Result-frame,
temporary-handle, callback, and partially built tree cleanup is tested for every
status path. Error-status result frames obey the same re-entrant lifetime and
explicit-release rules as successful frames.

## 6. Generated evidence and test plan

### 6.1 Surface snapshot and parity

A deterministic generator consumes the three pinned stubs and oracle Reflection
JSON. Checked-in manifests contain:

- all 51 exported DOM names and 50 canonical definitions;
- DOM aliases, methods, properties, constants, enum cases, interfaces, and
  modifiers;
- both DOM functions and 61 extension constants;
- the complete libxml and SimpleXML surfaces;
- exact parameter/default/tentative-return/deprecation metadata;
- object-handler and internal-extension operation mappings.

Parity tests compare compiler metadata and compiled-program Reflection against
the manifests. Generation is reproducible and `--check` fails on drift.

### 6.2 Upstream PHPT ledgers

The source contains:

- 868 DOM `.phpt` files (926 total files under `ext/dom/tests`);
- 32 libxml `.phpt` files (36 total files under `ext/libxml/tests`);
- 156 SimpleXML `.phpt` files (164 total files under
  `ext/simplexml/tests`).

All 1,056 PHPTs appear by relative path and source SHA-256 in checked-in ledgers.
Every entry is one of:

- `direct`: compile the PHPT `FILE`/`FILEEOF` body unchanged;
- `translated`: an equivalent Elephc fixture, with exact source test, fixture
  path, reason, and observation mapping;
- `not-applicable`: only a build/module harness probe with no DOM/libxml/
  SimpleXML observation.

Platform prerequisites, auxiliary files, environment, INI values, CLEAN
sections, and `EXPECT`/`EXPECTF`/`EXPECTREGEX` mode are recorded. Completion
requires no entry whose reason is a missing language/compiler/runtime feature
needed for the test's observable extension behaviour. Every security regression
is executable on each applicable supported target.

### 6.3 Differential and semantic tests

Focused fixtures run with the exact oracle and Elephc, comparing:

- stdout and stderr in order;
- exit status;
- thrown class/code/message;
- serialized bytes and written files;
- Reflection/debug output;
- post-success and post-failure tree state;
- libxml error buffers;
- wrapper identity observations.

Coverage includes every node kind and API family, constructors, custom
subclasses, dynamic properties, tree algorithms, namespaces, live/snapshot
collections, malformed corpora, encodings/BOMs, streams/entities, selectors,
XPath callbacks/re-entrancy, DTD/XSD/RNG, C14N, XInclude, SimpleXML object
handlers/interoperability, and legacy/modern differences.

### 6.4 Bridge, ownership, and adversarial tests

`elephc-dom` unit/integration tests cover:

- every handle kind and valid transition;
- forged, stale, cross-context, cross-document, and cross-family handles;
- request/result bounds, embedded NUL, nested values, overflow, ABI mismatch,
  unknown opcode, and double release;
- a request shorter than the eight-byte ABI prefix and a readable prefix whose
  declared header is unreadable; both assert `MALFORMED_REQUEST`, an initialized
  scalar result header when writable, no published payload pointers, and zero
  DOM mutation;
- structured invalid-release instrumentation for double, unknown, and foreign
  releases, asserting a production no-op and no effect on live results/context;
- independent re-entrant result frames;
- host callback throws and recursive DOM calls;
- panic injection and partial cleanup;
- libxml/Lexbor error and allocation-failure injection where supported;
- mutation epoch/live collection invalidation;
- parser limit and `PARSEHUGE` boundaries.

`tests/codegen/runtime_gc/` covers node/document/collection retention, detached
trees, weak-wrapper identity, custom-property cycles, object clone versus
`cloneNode`, import/adopt, SimpleXML views, registered subclasses, finalization,
and repeated create/destroy loops under heap debugging.

### 6.5 Compiler tests

Focused tests cover:

- name/catalog/extension detection and `--with-dom`;
- exact call planning for methods/functions, including named/spread/variadic
  calls and first-class callables;
- type checking, internal virtual properties, readonly errors, and diagnostics;
- EIR lowering/effects;
- target-aware bridge ABI materialization;
- Reflection and dynamic invocation;
- auto-link and force-link behaviour;
- missing/corrupt bridge diagnostics.

Any edited emitter retains aligned assembly comments. All new/touched Rust
files have the required module preamble and every explicit Rust function has a
specific Rustdoc docblock.

### 6.6 Examples and documentation

At minimum:

- `examples/dom-xml/main.php`: construction, namespaces, XPath, mutation,
  validation, and serialization;
- `examples/dom-html/main.php`: HTML5 parsing, selectors, `classList`,
  `innerHTML`, and serialization;
- `examples/dom-simplexml/main.php`: live bidirectional DOM/SimpleXML
  interoperability.

Each example has its required `.gitignore`. Public docs explain legacy/modern
selection, enabling/linking, streams, entity security, flags, encoding,
diagnostics, and standalone deployment. Internals docs explain native sources,
handles, wrapper identity, epochs, ABI, callbacks, and ownership.

### 6.7 Validation strategy

Implementation iterations run the smallest relevant crate/compiler/codegen/
error/GC filter and `git diff --check`. `cargo fmt` and `cargo fmt --all` are
never run in this repository because the repository contribution policy forbids
their broad mechanical rewrites. Style is maintained by focused manual
formatting of touched lines, compiler/test warnings, Rust module/function
documentation checks, assembly-comment checks, review, and
`git diff --check`; no whole-tree formatter is substituted.

Before implementation review:

- `cargo build`;
- focused complete DOM/libxml/SimpleXML compiler filters;
- `cargo test -p elephc-dom`;
- surface generator `--check`;
- all applicable 1,056 PHPT ledger entries;
- differential corpus;
- focused target-sensitive Linux x86_64 and ARM64 checks when needed;
- assembly-comment validation for touched emitters;
- `git diff --check`.

CI runs the complete extension filters, ledgers, differential corpus, and
ownership tests across macOS ARM64, Linux x86_64, and Linux ARM64. No target
inherits another target's result.

## 7. Delivery sequence

Implementation begins only after the specification lock and the repository's
required feature issue exists. The single-writer sequence is:

1. provenance snapshots, generators, and ledgers;
2. general internal-extension metadata/object-handler infrastructure;
3. vendored native build and versioned bridge ABI;
4. libxml error/stream surface;
5. authoritative tree, handles, wrappers, GC, and legacy XML DOM;
6. modern XML DOM and collections;
7. XPath, validators, XInclude, and C14N;
8. modern Lexbor HTML and selectors;
9. complete SimpleXML and bidirectional interoperability;
10. exhaustive PHPT/differential closure, examples, and docs;
11. final supported-target evidence and three-model implementation audit.

Each step is independently tested but is not called “DOM complete” until every
completion gate passes. If a prerequisite compiler/runtime defect is found, it
is fixed in Elephc in this branch with a focused regression test; PHP framework
or upstream fixture sources are not patched to hide it.

## 8. Review protocol and absolute consensus

The same three read-only Ollama reviewers are used for specification and code:

- `glm-5.2:cloud`;
- `kimi-k2.7-code:cloud`;
- `kimi-k3:cloud`.

The names identify Ollama model endpoints, including endpoints proxied by the
local Ollama service. Each reviewer receives the complete current artifact,
pinned provenance, oracle evidence, relevant php-src excerpts, and test
evidence. Reviews are independent.

Every finding is classified `BLOCKER`, `MAJOR`, `MINOR`, `NIT`, or `QUESTION`.
Absolute consensus means no open finding of any class and no unanswered
question. Conditional approval, “looks good except”, partial approval, and
silence are not locks.

Every proposed finding is checked against the pinned stub, source, PHPT, and
oracle before changing the artifact. A reviewer statement contradicted by those
sources is answered with evidence and does not mutate the specification merely
to obtain agreement.

After each material revision, all three reviewers re-review the complete
artifact. A valid approval is exactly:

`LOCK <artifact-kind> <sha256-or-commit>`

All three lines must name the same specification SHA-256 or implementation Git
commit. A later material change invalidates all locks. Prompts, full responses,
finding dispositions, and oracle evidence are retained under the review
evidence directory and summarized in the pull request.

## 9. Push and pull-request gate

The implementation commit may first be pushed to the fork and a draft pull
request opened only when:

1. it matches the locked specification;
2. the surface parity gate is exact;
3. the three ledgers have no semantic exclusions;
4. local build, focused tests, oracle differential checks, and hygiene pass;
5. examples and documentation are complete;
6. GLM 5.2, Kimi K2.7, and Kimi K3 lock the same implementation commit;
7. the worktree is clean and the fork destination/branch have been verified.

The draft PR then supplies the full supported-target CI evidence. A CI-driven
code change invalidates the three implementation locks and requires a complete
re-audit before the replacement commit is pushed. The PR is marked ready only
when the final locked commit has green macOS ARM64, Linux x86_64, and Linux
ARM64 CI and the remote branch resolves to that commit.

## Appendix A. Reproducible review evidence

This appendix records the evidence used to resolve review questions. The final
repository scripts reproduce it; the snippets below are observations, not a
replacement for the normative source order in section 2.

### A.1 Archive and engine verification

Downloaded archive verification:

```text
php-8.5.8.tar.xz
58910198d19e873048fe87cdfe16bc790025417ede3d1651bfa1c4b533d573f2

libxml2-2.15.3.tar.xz
78262a6e7ac170d6528ebfe2efccdf220191a5af6a6cd61ea4a9a9a5042c7a07
```

Oracle output after building PHP against that libxml2:

```text
8.5.8|2.15.3|21503
```

The static libxml2 feature probe reports:

```text
Threads Tree Output Push Reader Patterns Writer SAXv1 DTDValid HTML C14N
Catalog XPath XPointer XInclude Iconv ISO8859X Regexps Automata RelaxNG
Schemas
```

`ext/lexbor/patches/README.md` at the pinned PHP commit states:

```text
The current Lexbor version is 2.7.0.
```

### A.2 Source-tree and PHPT verification

The following commands are run in the sparse checkout of the peeled commit:

```text
find ext/dom/tests -type f -name '*.phpt'        -> 868
find ext/dom/tests -type f                       -> 926
find ext/libxml/tests -type f -name '*.phpt'     -> 32
find ext/libxml/tests -type f                    -> 36
find ext/simplexml/tests -type f -name '*.phpt'  -> 156
find ext/simplexml/tests -type f                 -> 164
```

The resulting test-tree objects and sorted content digests are frozen in
section 2.1.

### A.3 DOM Reflection and stub verification

The exact oracle aggregation prints:

```text
exported_type_names=51
canonical_type_definitions=50
direct_methods=313
direct_properties_across_exported_names=185
canonical_direct_properties=184
class_or_enum_constants=16
extension_constants=61
functions=2
```

Alias evidence:

```text
ReflectionClass("Dom\DomException")->getName() = DOMException
ReflectionExtension("dom") key dom\domexception -> canonical DOMException
php_dom_arginfo.h:
zend_register_class_alias("Dom\\DOMException", class_entry);
```

The exact PHP 8.5.8 Reflection probe also resolves the apparent property-count
discrepancy. `DOMException::$code` is directly declared by `DOMException`, not
inherited from `Exception`, and the exported alias encounters that same direct
property a second time:

```text
DOMException canonical=DOMException
message:Exception
file:Exception
line:Exception
code:DOMException
Dom\DomException canonical=DOMException
message:Exception
file:Exception
line:Exception
code:DOMException
```

The pinned stub declarations are:

```text
function dom_import_simplexml(object $node): DOMAttr|DOMElement
function Dom\import_simplexml(object $node): Dom\Attr|Dom\Element
readonly final class Dom\NamespaceInfo
final class Dom\XPath
class Dom\Entity extends Dom\Node
class Dom\EntityReference extends Dom\Node
class Dom\Notation extends Dom\Node
Dom\Element::$substitutedNodeValue is public string, virtual
```

The relevant oracle modifier probe is:

```text
Dom\NamespaceInfo final=yes abstract=no readonly=yes
Dom\XPath         final=yes abstract=no readonly=no
Dom\Document      final=no  abstract=yes
Dom\Node          final=no  abstract=no
Dom\Element       final=no  abstract=no
Dom\Attr          final=no  abstract=no
Dom\Text          final=no  abstract=no
Dom\CharacterData final=no  abstract=no
Dom\HTMLCollection final=no abstract=no
Dom\CDATASection  final=no  abstract=no
```

The two import functions have one required parameter named `node` in the
pinned executable; neither exposes an optional subclass-name parameter:

```text
dom_import_simplexml parameters=1 required=1 names=[node]
Dom\import_simplexml parameters=1 required=1 names=[node]
```

Virtual-property evidence:

```text
DOMNode::$nodeName isVirtual=yes getHooks=[]
Dom\Node::$nodeName isVirtual=yes getHooks=[]
```

The following probe was executed by the PHP 8.5.8 CLI built from peeled commit
`26b97507444c4fbda072f57dda1820f7b7d5e467`. Its error handler prints the
numeric severity and exact diagnostic before reading each newly created
property back:

```text
8.5.8
8192:Creation of dynamic property DOMDocument::$reviewMarker is deprecated
DOMDocument:legacy
8192:Creation of dynamic property Dom\XMLDocument::$reviewMarker is deprecated
Dom\XMLDocument:modern
```

The same pinned executable assigns type-correct values to every declared
`LibXMLError` property and immediately reads them back without a warning or
exception:

```text
int(1)                       # level
int(2)                       # code
int(3)                       # column
string(7) "message"          # message
string(8) "file.xml"         # file
int(4)                       # line
```

The source-side checks for the two disputed modern surface claims are also
direct: `ext/dom/php_dom.stub.php` declares `Dom\Entity`,
`Dom\EntityReference`, and `Dom\Notation`, and declares the public virtual
string property `Dom\Element::$substitutedNodeValue`. The matching upstream
behaviour test is
`ext/dom/tests/modern/extensions/Element_substitutedNodeValue.phpt`.

### A.4 SimpleXML family evidence

The oracle and the two named upstream PHPTs agree:

```text
same-family legacy re-import === true
legacy then modern:
TypeError|Dom\import_simplexml(): Argument #1 ($node) must not be already imported as a DOMNode

same-family modern re-import === true
modern then legacy:
TypeError|dom_import_simplexml(): Argument #1 ($node) must not be already imported as a Dom\Node
```

### A.5 Reviewer endpoint evidence

The local Ollama registry used for this review lists these exact callable model
identifiers:

```text
glm-5.2:cloud
kimi-k2.7-code:cloud
kimi-k3:cloud
```

The human-facing names remain GLM 5.2, Kimi K2.7, and Kimi K3; endpoint
identifiers are deliberately lowercase where Ollama defines them that way.
