//! Purpose:
//! The PHP form of the classic DOM prelude: the model the declarations in `build` were
//! transcribed from, kept under `cfg(test)` as the node-by-node parse-parity oracle.
//!
//! Called from:
//! - `crate::dom_prelude::oracle`.
//!
//! Key details:
//! - The source was verified against php's own `DOMDocument` family (php 8.5, libxml 2.9.13)
//!   before it was transcribed: tree building and mutation by a randomized operation fuzzer,
//!   HTML parsing and serialization by ~10,000 generated documents compared for identical
//!   trees, `saveHTML()` and `saveXML()` bytes, plus targeted cases for the XML parser and for
//!   exception ordering. Edit this model and re-transcribe rather than editing `build` by hand.
//! - Class names are the real ones (`DOMNode`, ...): the prelude lives in the global namespace,
//!   so unlike the `Dom\` prelude no name mangling is needed.

pub(super) const SRC: &str = r####"<?php
const LIBXML_VERSION = 20913;
const LIBXML_DOTTED_VERSION = '2.9.13';
const LIBXML_LOADED_VERSION = 20913;
const LIBXML_RECOVER = 1;
const LIBXML_NOENT = 2;
const LIBXML_DTDLOAD = 4;
const LIBXML_DTDATTR = 8;
const LIBXML_DTDVALID = 16;
const LIBXML_NOERROR = 32;
const LIBXML_NOWARNING = 64;
const LIBXML_PEDANTIC = 128;
const LIBXML_NOBLANKS = 256;
const LIBXML_XINCLUDE = 1024;
const LIBXML_NSCLEAN = 8192;
const LIBXML_NOCDATA = 16384;
const LIBXML_NOXMLDECL = 2;
const LIBXML_PARSEHUGE = 524288;
const LIBXML_BIGLINES = 4194304;
const LIBXML_NOEMPTYTAG = 4;
const LIBXML_SCHEMA_CREATE = 1;
const LIBXML_HTML_NOIMPLIED = 8192;
const LIBXML_HTML_NODEFDTD = 4;
const XML_ELEMENT_NODE = 1;
const XML_ATTRIBUTE_NODE = 2;
const XML_TEXT_NODE = 3;
const XML_CDATA_SECTION_NODE = 4;
const XML_ENTITY_REF_NODE = 5;
const XML_ENTITY_NODE = 6;
const XML_PI_NODE = 7;
const XML_COMMENT_NODE = 8;
const XML_DOCUMENT_NODE = 9;
const XML_DOCUMENT_TYPE_NODE = 10;
const XML_DOCUMENT_FRAG_NODE = 11;
const XML_NOTATION_NODE = 12;
const XML_HTML_DOCUMENT_NODE = 13;
const XML_DTD_NODE = 14;
const XML_ELEMENT_DECL_NODE = 15;
const XML_ATTRIBUTE_DECL_NODE = 16;
const XML_ENTITY_DECL_NODE = 17;
const XML_NAMESPACE_DECL_NODE = 18;
const XML_LOCAL_NAMESPACE = 18;
const XML_ATTRIBUTE_CDATA = 1;
const XML_ATTRIBUTE_ID = 2;
const XML_ATTRIBUTE_IDREF = 3;
const XML_ATTRIBUTE_IDREFS = 4;
const XML_ATTRIBUTE_ENTITY = 6;
const XML_ATTRIBUTE_NMTOKEN = 7;
const XML_ATTRIBUTE_NMTOKENS = 8;
const XML_ATTRIBUTE_ENUMERATION = 9;
const XML_ATTRIBUTE_NOTATION = 10;
const DOM_PHP_ERR = 0;
const DOM_INDEX_SIZE_ERR = 1;
const DOMSTRING_SIZE_ERR = 2;
const DOM_HIERARCHY_REQUEST_ERR = 3;
const DOM_WRONG_DOCUMENT_ERR = 4;
const DOM_INVALID_CHARACTER_ERR = 5;
const DOM_NO_DATA_ALLOWED_ERR = 6;
const DOM_NO_MODIFICATION_ALLOWED_ERR = 7;
const DOM_NOT_FOUND_ERR = 8;
const DOM_NOT_SUPPORTED_ERR = 9;
const DOM_INUSE_ATTRIBUTE_ERR = 10;
const DOM_INVALID_STATE_ERR = 11;
const DOM_SYNTAX_ERR = 12;
const DOM_INVALID_MODIFICATION_ERR = 13;
const DOM_NAMESPACE_ERR = 14;
const DOM_INVALID_ACCESS_ERR = 15;
const DOM_VALIDATION_ERR = 16;

class DOMException extends \Exception
{
}

class DOMNodeList implements \IteratorAggregate, \Countable
{
    public int $length = 0;
    public ?DOMNode $parent = null;
    public ?array $items = null;

    public function __construct(?DOMNode $parent = null, array $items = [])
    {
        $this->parent = $parent;
        $this->items = $items === [] ? null : $items;
        $this->length = count($items);
    }

    public function item(int $index): ?DOMNode
    {
        if ($index < 0) {
            return null;
        }
        if ($this->parent === null) {
            return $this->items[$index] ?? null;
        }
        $node = $this->parent->firstChild;
        while ($node !== null && $index > 0) {
            $node = $node->nextSibling;
            $index--;
        }
        return $node;
    }

    public function count(): int
    {
        return $this->length;
    }

    public function toArray(): array
    {
        if ($this->parent === null) {
            return $this->items ?? [];
        }
        $all = [];
        $node = $this->parent->firstChild;
        while ($node !== null) {
            $all[] = $node;
            $node = $node->nextSibling;
        }
        return $all;
    }

    public function getIterator(): \Iterator
    {
        return new \ArrayIterator($this->toArray());
    }
}

class DOMNamedNodeMap implements \IteratorAggregate, \Countable
{
    public int $length = 0;
    public ?DOMElement $owner = null;

    public function __construct(?DOMElement $owner = null)
    {
        $this->owner = $owner;
    }

    public function getNamedItem(string $qualifiedName): ?DOMNode
    {
        if ($this->owner === null) {
            return null;
        }
        foreach ($this->owner->attrNodes as $attr) {
            if ($attr->name === $qualifiedName) {
                return $attr;
            }
        }
        return null;
    }

    public function getNamedItemNS(?string $namespace, string $localName): ?DOMNode
    {
        if ($this->owner === null) {
            return null;
        }
        return $this->owner->getAttributeNodeNS($namespace, $localName);
    }

    public function item(int $index): ?DOMNode
    {
        if ($this->owner === null || $index < 0) {
            return null;
        }
        $nodes = array_values($this->owner->attrNodes);
        return $nodes[$index] ?? null;
    }

    public function count(): int
    {
        return $this->length;
    }

    public function getIterator(): \Iterator
    {
        if ($this->owner === null) {
            return new \ArrayIterator([]);
        }
        $byName = [];
        foreach ($this->owner->attrNodes as $attr) {
            $byName[$attr->name] = $attr;
        }
        return new \ArrayIterator($byName);
    }
}

class DOMNode
{
    public string $nodeName = '';
    public ?string $nodeValue = null;
    public int $nodeType = 0;
    public ?DOMNode $parentNode = null;
    public DOMNodeList $childNodes;
    public ?DOMNode $firstChild = null;
    public ?DOMNode $lastChild = null;
    public ?DOMNode $previousSibling = null;
    public ?DOMNode $nextSibling = null;
    public ?DOMNamedNodeMap $attributes = null;
    public ?DOMDocument $ownerDocument = null;
    public ?string $namespaceURI = null;
    public string $prefix = '';
    public ?string $localName = null;
    public ?string $baseURI = null;
    public ?string $textContent = '';
    public string $textCache = '';
    public string $tagName = '';
    public ?string $data = null;
    public ?string $value = null;
    public string $name = '';
    public string $target = '';
    public ?DOMElement $ownerElement = null;

    public function __construct()
    {
        $this->childNodes = new DOMNodeList($this);
    }

    public function __get(string $name): mixed
    {
        if ($name === 'firstElementChild') {
            $node = $this->firstChild;
            while ($node !== null && $node->nodeType !== 1) {
                $node = $node->nextSibling;
            }
            return $node;
        }
        if ($name === 'lastElementChild') {
            $node = $this->lastChild;
            while ($node !== null && $node->nodeType !== 1) {
                $node = $node->previousSibling;
            }
            return $node;
        }
        if ($name === 'childElementCount') {
            $count = 0;
            $node = $this->firstChild;
            while ($node !== null) {
                if ($node->nodeType === 1) {
                    $count++;
                }
                $node = $node->nextSibling;
            }
            return $count;
        }
        if ($name === 'previousElementSibling') {
            $node = $this->previousSibling;
            while ($node !== null && $node->nodeType !== 1) {
                $node = $node->previousSibling;
            }
            return $node;
        }
        if ($name === 'nextElementSibling') {
            $node = $this->nextSibling;
            while ($node !== null && $node->nodeType !== 1) {
                $node = $node->nextSibling;
            }
            return $node;
        }
        return null;
    }

    public function __isset(string $name): bool
    {
        if ($name === 'firstElementChild' || $name === 'lastElementChild' || $name === 'previousElementSibling' || $name === 'nextElementSibling') {
            return $this->__get($name) !== null;
        }
        return $name === 'childElementCount';
    }

    // Recomputes this node's cached text from its children.
    public function computeText(): void
    {
        $type = $this->nodeType;
        if ($type !== 1 && $type !== 9 && $type !== 11 && $type !== 13) {
            return;
        }
        $text = '';
        $child = $this->firstChild;
        while ($child !== null) {
            $kind = $child->nodeType;
            if ($kind === 3 || $kind === 4 || $kind === 1) {
                $text .= $child->textContent;
            }
            $child = $child->nextSibling;
        }
        if ($type === 1) {
            $this->nodeValue = $text;
        } else {
            $this->textContent = $text;
        }
        $this->textCache = $text;
    }

    // Recomputes the cached text of this node and of every ancestor after its subtree changed.
    public function refreshText(): void
    {
        $node = $this;
        while ($node !== null) {
            $node->computeText();
            $node = $node->parentNode;
        }
    }

    // True while a parser fills the tree and the per-link text refresh is deferred to its end.
    public function isBuilding(): bool
    {
        $doc = $this->nodeType === 9 || $this->nodeType === 13 ? $this : $this->ownerDocument;
        return $doc !== null && $doc->building;
    }

    // Applies assignments made straight to nodeValue / textContent of an element or document.
    public function syncText(): void
    {
        $type = $this->nodeType;
        if ($type === 1 || $type === 9 || $type === 11 || $type === 13) {
            $current = $type === 1 ? $this->nodeValue : $this->textContent;
            if ($current !== $this->textCache) {
                $this->replaceWithText($current === null ? '' : $current);
            }
            $child = $this->firstChild;
            while ($child !== null) {
                if ($child->nodeType === 1) {
                    $child->syncText();
                }
                $child = $child->nextSibling;
            }
        }
    }

    public function replaceWithText(string $value): void
    {
        $doc = $this->owningDocument();
        if ($doc !== null) {
            $doc->building = true;
        }
        while ($this->firstChild !== null) {
            $this->firstChild->detach();
        }
        if ($value !== '') {
            $text = new DOMText($value);
            $text->ownerDocument = $doc;
            $this->linkBefore($text, null);
        }
        if ($doc !== null) {
            $doc->building = false;
        }
        $this->refreshText();
    }

    public function owningDocument(): ?DOMDocument
    {
        if ($this instanceof DOMDocument) {
            return $this;
        }
        return $this->ownerDocument;
    }

    public function setOwner(?DOMDocument $document): void
    {
        $this->ownerDocument = $document;
        $node = $this->firstChild;
        while ($node !== null) {
            $node->setOwner($document);
            $node = $node->nextSibling;
        }
        if ($this instanceof DOMElement) {
            foreach ($this->attrNodes as $attr) {
                $attr->setOwner($document);
            }
        }
    }

    public function linkBefore(DOMNode $node, ?DOMNode $ref): void
    {
        $node->parentNode = $this;
        $prev = $ref === null ? $this->lastChild : $ref->previousSibling;
        $node->previousSibling = $prev;
        $node->nextSibling = $ref;
        if ($prev === null) {
            $this->firstChild = $node;
        } else {
            $prev->nextSibling = $node;
        }
        if ($ref === null) {
            $this->lastChild = $node;
        } else {
            $ref->previousSibling = $node;
        }
        $this->bumpCount(1);
        $this->childrenChanged();
        $kind = $node->nodeType;
        if (($kind === 3 || $kind === 4 || $kind === 1) && $node->textContent !== '' && !$this->isBuilding()) {
            $this->refreshText();
        }
    }

    public function detach(): void
    {
        $parent = $this->parentNode;
        if ($parent === null) {
            return;
        }
        $prev = $this->previousSibling;
        $next = $this->nextSibling;
        if ($prev === null) {
            $parent->firstChild = $next;
        } else {
            $prev->nextSibling = $next;
        }
        if ($next === null) {
            $parent->lastChild = $prev;
        } else {
            $next->previousSibling = $prev;
        }
        $this->parentNode = null;
        $this->previousSibling = null;
        $this->nextSibling = null;
        $parent->bumpCount(-1);
        $parent->childrenChanged();
        $kind = $this->nodeType;
        if (($kind === 3 || $kind === 4 || $kind === 1) && $this->textContent !== '' && !$parent->isBuilding()) {
            $parent->refreshText();
        }
    }

    public function childrenChanged(): void
    {
    }

    // Adjusts the live child count; written through a local because a chained store leaks a reference.
    public function bumpCount(int $by): void
    {
        $list = $this->childNodes;
        $list->length = $list->length + $by;
    }

    public function checkInsert(DOMNode $node, ?DOMNode $child): void
    {
        if ($node->nodeType === 2) {
            throw new DOMException('Hierarchy Request Error', 3);
        }
        if ($node->nodeType === 9 || $node->nodeType === 13) {
            if ($node === $this->owningDocument()) {
                throw new DOMException('Hierarchy Request Error', 3);
            }
            throw new DOMException('Wrong Document Error', 4);
        }
        $walk = $this;
        while ($walk !== null) {
            if ($walk === $node) {
                throw new DOMException('Hierarchy Request Error', 3);
            }
            $walk = $walk->parentNode;
        }
        $mine = $this->owningDocument();
        $theirs = $node->owningDocument();
        if ($mine !== null && $theirs !== null && $mine !== $theirs) {
            throw new DOMException('Wrong Document Error', 4);
        }
        if ($child !== null && $child->parentNode !== $this) {
            throw new DOMException('Not Found Error', 8);
        }
    }

    public function appendChild(DOMNode $node): DOMNode
    {
        return $this->insertBefore($node, null);
    }

    public function insertBefore(DOMNode $node, ?DOMNode $child = null): DOMNode
    {
        if ($this->nodeType !== 1 && $this->nodeType !== 9 && $this->nodeType !== 11 && $this->nodeType !== 13) {
            return $node;
        }
        if ($node instanceof DOMAttr && $this instanceof DOMElement) {
            if ($child !== null && $child->parentNode !== $this) {
                throw new DOMException('Not Found Error', 8);
            }
            $this->setAttributeNode($node);
            return $node;
        }
        $this->checkInsert($node, null);
        if ($node->nodeType === 11 && $node->firstChild === null) {
            return $node;
        }
        if ($child !== null && $child->parentNode !== $this) {
            throw new DOMException('Not Found Error', 8);
        }
        if ($child === $node) {
            $node->detach();
            throw new \Error('Cannot add newnode as the previous sibling of refnode');
        }
        $mine = $this->owningDocument();
        if ($node->nodeType === 11) {
            $first = $node->firstChild;
            while ($node->firstChild !== null) {
                $moved = $node->firstChild;
                $moved->detach();
                if ($mine !== null) {
                    $moved->setOwner($mine);
                }
                $this->linkBefore($moved, $child);
            }
            return $first;
        }
        $node->detach();
        if ($mine !== null && $node->ownerDocument === null) {
            $node->setOwner($mine);
        }
        $this->linkBefore($node, $child);
        return $node;
    }

    public function removeChild(DOMNode $child): DOMNode
    {
        if ($child->parentNode !== $this) {
            throw new DOMException('Not Found Error', 8);
        }
        $child->detach();
        return $child;
    }

    public function replaceChild(DOMNode $node, DOMNode $child): DOMNode
    {
        $mine = $this->owningDocument();
        $theirs = $node->owningDocument();
        if ($mine !== null && $theirs !== null && $mine !== $theirs) {
            throw new DOMException('Wrong Document Error', 4);
        }
        if ($this->firstChild === null) {
            return $child;
        }
        $this->checkInsert($node, null);
        if ($child->parentNode !== $this) {
            throw new DOMException('Not Found Error', 8);
        }
        if ($node === $child) {
            return $child;
        }
        $this->insertBefore($node, $child);
        $child->detach();
        return $child;
    }

    public function hasChildNodes(): bool
    {
        return $this->firstChild !== null;
    }

    public function hasAttributes(): bool
    {
        return false;
    }

    public function isSameNode(DOMNode $otherNode): bool
    {
        return $this === $otherNode;
    }

    public function contains(DOMNode $other): bool
    {
        $walk = $other;
        while ($walk !== null) {
            if ($walk === $this) {
                return true;
            }
            $walk = $walk->parentNode;
        }
        return false;
    }

    public function getRootNode(array $options = []): DOMNode
    {
        $_unused = $options;
        $root = $this;
        while ($root->parentNode !== null) {
            $root = $root->parentNode;
        }
        return $root;
    }

    public function getLineNo(): int
    {
        return 0;
    }

    public function isDefaultNamespace(string $namespace): bool
    {
        return $this->lookupNamespaceURI(null) === $namespace;
    }

    public function lookupNamespaceURI(?string $prefix): ?string
    {
        if ($prefix === 'xml') {
            return 'http://www.w3.org/XML/1998/namespace';
        }
        $node = $this;
        while ($node !== null) {
            if ($node instanceof DOMElement) {
                if ($node->nsDecl !== []) {
                    $key = $prefix === null ? '' : $prefix;
                    if (isset($node->nsDecl[$key])) {
                        return $node->nsDecl[$key];
                    }
                }
                if ($node->namespaceURI !== null && $node->prefix === ($prefix === null ? '' : $prefix)) {
                    return $node->namespaceURI;
                }
            }
            $node = $node->parentNode;
        }
        return null;
    }

    public function lookupPrefix(string $namespace): ?string
    {
        $node = $this;
        while ($node !== null) {
            if ($node instanceof DOMElement) {
                foreach ($node->nsDecl as $declared => $uri) {
                    if ($uri === $namespace) {
                        return (string) $declared;
                    }
                }
                if ($node->namespaceURI === $namespace && $node->prefix !== '') {
                    return $node->prefix;
                }
            }
            $node = $node->parentNode;
        }
        return null;
    }

    public function normalize(): void
    {
        $node = $this->firstChild;
        while ($node !== null) {
            $next = $node->nextSibling;
            if ($node->nodeType === 3) {
                while ($next !== null && $next->nodeType === 3) {
                    // appendData(), not a store through an instanceof-narrowed local: that store leaks the node.
                    $node->appendData($next->data);
                    $following = $next->nextSibling;
                    $next->detach();
                    $next = $following;
                }
                if ($node->data === '') {
                    $node->detach();
                }
            } elseif ($node->nodeType === 1) {
                $node->normalize();
            }
            $node = $next;
        }
    }

    public function getNodePath(): ?string
    {
        if ($this->parentNode === null) {
            return $this instanceof DOMDocument ? '/' : null;
        }
        $parentPath = $this->parentNode->getNodePath();
        $step = $this->nodeName;
        if ($this->nodeType === 3 || $this->nodeType === 4) {
            $step = 'text()';
        } elseif ($this->nodeType === 8) {
            $step = 'comment()';
        } elseif ($this->nodeType === 7) {
            $step = 'processing-instruction(\'' . $this->nodeName . '\')';
        }
        $same = 0;
        $index = 0;
        $sibling = $this->parentNode->firstChild;
        while ($sibling !== null) {
            $alike = $sibling->nodeType === $this->nodeType || (($sibling->nodeType === 3 || $sibling->nodeType === 4) && ($this->nodeType === 3 || $this->nodeType === 4));
            if ($alike && ($this->nodeType !== 1 || $sibling->nodeName === $this->nodeName)) {
                $same++;
                if ($sibling === $this) {
                    $index = $same;
                }
            }
            $sibling = $sibling->nextSibling;
        }
        $suffix = $same > 1 ? '[' . $index . ']' : '';
        if ($parentPath === '/') {
            return '/' . $step . $suffix;
        }
        return $parentPath . '/' . $step . $suffix;
    }

    public function cloneNode(bool $deep = false): DOMNode
    {
        $this->syncText();
        $copy = $this->shallowCopy();
        $home = $this->ownerDocument;
        $copy->setOwner($home);
        if ($deep) {
            $doc = $this->owningDocument();
            $was = $doc !== null && $doc->building;
            if ($doc !== null) {
                $doc->building = true;
            }
            $child = $this->firstChild;
            while ($child !== null) {
                $part = $child->cloneNode(true);
                $last = $copy->lastChild;
                if ($part->nodeType === 3 && $last !== null && $last->nodeType === 3) {
                    $last->appendData($part->data);
                } else {
                    $copy->linkBefore($part, null);
                }
                $child = $child->nextSibling;
            }
            if ($doc !== null) {
                $doc->building = $was;
            }
            $copy->computeText();
        }
        return $copy;
    }

    public function shallowCopy(): DOMNode
    {
        return new DOMNode();
    }

    public function C14N(bool $exclusive = false, bool $withComments = false, ?array $xpath = null, ?array $nsPrefixes = null): string|false
    {
        $_unused = [$exclusive, $withComments, $xpath, $nsPrefixes];
        return __ElephcDomWriter::xml($this, false, false, 0);
    }
}

class DOMCharacterData extends DOMNode
{
    public function __get(string $name): mixed
    {
        if ($name === 'length') {
            return __ElephcDom::charLength($this->data);
        }
        return parent::__get($name);
    }

    public function __isset(string $name): bool
    {
        return $name === 'length' || parent::__isset($name);
    }

    // One storage cell serves data, nodeValue and textContent, so assigning any of them is seen by all.
    public function bindData(string $data): void
    {
        $this->data = $data;
        $this->nodeValue = &$this->data;
        $this->textContent = &$this->data;
    }

    public function dataChanged(): void
    {
        if ($this->parentNode !== null && !$this->isBuilding()) {
            $this->parentNode->refreshText();
        }
    }

    public function appendData(mixed $data): bool
    {
        $data = (string) $data;
        $this->data = $this->data . $data;
        $this->dataChanged();
        return true;
    }

    public function substringData(int $offset, int $count): string
    {
        $length = __ElephcDom::charLength($this->data);
        if ($offset < 0 || $offset > $length || $count < 0) {
            throw new DOMException('Index Size Error', 1);
        }
        return __ElephcDom::charSlice($this->data, $offset, $count);
    }

    public function insertData(int $offset, mixed $data): bool
    {
        $data = (string) $data;
        $length = __ElephcDom::charLength($this->data);
        if ($offset < 0 || $offset > $length) {
            throw new DOMException('Index Size Error', 1);
        }
        $this->data = __ElephcDom::charSlice($this->data, 0, $offset) . $data . __ElephcDom::charSlice($this->data, $offset, $length);
        $this->dataChanged();
        return true;
    }

    public function deleteData(int $offset, int $count): bool
    {
        $length = __ElephcDom::charLength($this->data);
        if ($offset < 0 || $offset > $length || $count < 0) {
            throw new DOMException('Index Size Error', 1);
        }
        $this->data = __ElephcDom::charSlice($this->data, 0, $offset) . __ElephcDom::charSlice($this->data, $offset + $count, $length);
        $this->dataChanged();
        return true;
    }

    public function replaceData(int $offset, int $count, mixed $data): bool
    {
        $data = (string) $data;
        $length = __ElephcDom::charLength($this->data);
        if ($offset < 0 || $offset > $length || $count < 0) {
            throw new DOMException('Index Size Error', 1);
        }
        $this->data = __ElephcDom::charSlice($this->data, 0, $offset) . $data . __ElephcDom::charSlice($this->data, $offset + $count, $length);
        $this->dataChanged();
        return true;
    }
}

class DOMText extends DOMCharacterData
{
    public function __construct(mixed $data = '')
    {
        $data = (string) $data;
        parent::__construct();
        $this->nodeType = 3;
        $this->nodeName = '#text';
        $this->bindData($data);
    }

    public function __get(string $name): mixed
    {
        if ($name === 'wholeText') {
            $start = $this;
            while ($start->previousSibling !== null && ($start->previousSibling->nodeType === 3 || $start->previousSibling->nodeType === 4)) {
                $start = $start->previousSibling;
            }
            $text = '';
            while ($start !== null && ($start->nodeType === 3 || $start->nodeType === 4)) {
                $text .= $start->data;
                $start = $start->nextSibling;
            }
            return $text;
        }
        return parent::__get($name);
    }

    public function isWhitespaceInElementContent(): bool
    {
        return trim($this->data, " \t\r\n") === '';
    }

    public function isElementContentWhitespace(): bool
    {
        return trim($this->data, " \t\r\n") === '';
    }

    public function splitText(int $offset): DOMText
    {
        $length = __ElephcDom::charLength($this->data);
        if ($offset < 0 || $offset > $length) {
            throw new DOMException('Index Size Error', 1);
        }
        $tail = __ElephcDom::charSlice($this->data, $offset, $length);
        $this->data = __ElephcDom::charSlice($this->data, 0, $offset);
        $new = $this->nodeType === 4 ? new DOMCdataSection($tail) : new DOMText($tail);
        $new->ownerDocument = $this->ownerDocument;
        if ($this->parentNode !== null) {
            $after = $this->nextSibling;
            $this->parentNode->linkBefore($new, $after);
        }
        return $new;
    }

    public function shallowCopy(): DOMNode
    {
        return new DOMText($this->data);
    }
}

class DOMComment extends DOMCharacterData
{
    public function __construct(mixed $data = '')
    {
        $data = (string) $data;
        parent::__construct();
        $this->nodeType = 8;
        $this->nodeName = '#comment';
        $this->bindData($data);
    }

    public function shallowCopy(): DOMNode
    {
        return new DOMComment($this->data);
    }
}

class DOMCdataSection extends DOMText
{
    public function __construct(mixed $data = '')
    {
        $data = (string) $data;
        parent::__construct($data);
        $this->nodeType = 4;
        $this->nodeName = '#cdata-section';
    }

    public function shallowCopy(): DOMNode
    {
        return new DOMCdataSection($this->data);
    }
}

class DOMProcessingInstruction extends DOMNode
{

    public function __construct(string $name, mixed $value = '')
    {
        $value = (string) $value;
        parent::__construct();
        $this->nodeType = 7;
        $this->nodeName = $name;
        $this->target = $name;
        $this->data = $value;
        $this->nodeValue = &$this->data;
        $this->textContent = &$this->data;
    }

    public function shallowCopy(): DOMNode
    {
        return new DOMProcessingInstruction($this->target, $this->data);
    }
}

class DOMAttr extends DOMNode
{
    public bool $specified = true;
    public bool $valueless = false;
    public bool $schemaTypeInfo = false;

    public function __construct(string $name, mixed $value = '')
    {
        $value = (string) $value;
        parent::__construct();
        $this->nodeType = 2;
        $this->name = $name;
        $this->nodeName = $name;
        $this->value = $value;
        $this->nodeValue = &$this->value;
        $this->textContent = &$this->value;
        $colon = strpos($name, ':');
        if ($colon !== false) {
            $this->prefix = substr($name, 0, $colon);
            $this->localName = substr($name, $colon + 1);
        } else {
            $this->localName = $name;
        }
    }

    // Binds the attribute to a namespace: `name` becomes the local part, `nodeName` stays qualified.
    public function applyNamespace(string $uri): void
    {
        $this->namespaceURI = $uri;
        if ($this->localName !== null && $this->prefix !== '') {
            $this->name = $this->localName;
        }
    }

    public function isId(): bool
    {
        return $this->name === 'id' && $this->ownerDocument !== null && $this->ownerDocument->isHtml;
    }

    public function shallowCopy(): DOMNode
    {
        $copy = new DOMAttr($this->nodeName, $this->value);
        $copy->namespaceURI = $this->namespaceURI;
        $copy->prefix = $this->prefix;
        $copy->localName = $this->localName;
        $copy->name = $this->name;
        return $copy;
    }
}

class DOMElement extends DOMNode
{
    public array $attrNodes = [];
    public array $nsDecl = [];
    public array $nsAuto = [];
    public string $schemaTypeInfo = '';

    public function __construct(string $qualifiedName, mixed $value = null, string $namespace = '')
    {
        $value = $value === null ? null : (string) $value;
        parent::__construct();
        if (!__ElephcDom::validName($qualifiedName)) {
            throw new DOMException('Invalid Character Error', 5);
        }
        $this->nodeType = 1;
        $this->nodeName = $qualifiedName;
        $this->tagName = $qualifiedName;
        $this->nodeValue = '';
        $this->textContent = &$this->nodeValue;
        $this->attributes = new DOMNamedNodeMap($this);
        $colon = strpos($qualifiedName, ':');
        if ($colon !== false) {
            $this->prefix = substr($qualifiedName, 0, $colon);
            $this->localName = substr($qualifiedName, $colon + 1);
        } else {
            $this->localName = $qualifiedName;
        }
        if ($namespace !== '') {
            $this->namespaceURI = $namespace;
            $this->nsDecl[$this->prefix] = $namespace;
            $this->nsAuto[$this->prefix] = true;
        }
        if ($value !== null && $value !== '') {
            $this->linkBefore(new DOMText($value), null);
        }
    }

    public function hasAttributes(): bool
    {
        return $this->attrNodes !== [];
    }

    public function syncAttrCount(): void
    {
        $map = $this->attributes;
        if ($map !== null) {
            $map->length = count($this->attrNodes);
        }
    }

    public function lookupAttrKey(string $name): string
    {
        return $name;
    }

    public function getAttribute(string $qualifiedName): string
    {
        $key = $this->lookupAttrKey($qualifiedName);
        if (isset($this->attrNodes[$key])) {
            return $this->attrNodes[$key]->value;
        }
        if (str_starts_with($qualifiedName, 'xmlns')) {
            $declared = $qualifiedName === 'xmlns' ? '' : substr($qualifiedName, 6);
            if (isset($this->nsDecl[$declared])) {
                return $this->nsDecl[$declared];
            }
        }
        return '';
    }

    public function getAttributeNode(string $qualifiedName): DOMAttr|false
    {
        $key = $this->lookupAttrKey($qualifiedName);
        if (isset($this->attrNodes[$key])) {
            return $this->attrNodes[$key];
        }
        return false;
    }

    public function setAttribute(string $qualifiedName, mixed $value): DOMAttr
    {
        $value = (string) $value;
        if (!__ElephcDom::validName($qualifiedName)) {
            throw new DOMException('Invalid Character Error', 5);
        }
        $key = $this->lookupAttrKey($qualifiedName);
        if (isset($this->attrNodes[$key])) {
            $existing = $this->attrNodes[$key];
            $existing->value = $value;
            $existing->valueless = false;
            return $existing;
        }
        $attr = new DOMAttr($key, $value);
        if ($attr->prefix !== '') {
            $bound = $this->lookupNamespaceURI($attr->prefix);
            if ($bound !== null) {
                $attr->applyNamespace($bound);
            }
        }
        $attr->ownerElement = $this;
        $attr->ownerDocument = $this->ownerDocument;
        $attr->parentNode = $this;
        $this->attrNodes[$key] = $attr;
        $this->syncAttrCount();
        return $attr;
    }

    public function setAttributeNode(DOMAttr $attr): ?DOMAttr
    {
        $old = $this->attrNodes[$attr->nodeName] ?? null;
        if ($old === $attr) {
            return null;
        }
        if ($attr->ownerElement !== null) {
            throw new DOMException('Inuse Attribute Error', 10);
        }
        $mine = $this->owningDocument();
        if ($mine !== null && $attr->ownerDocument !== null && $attr->ownerDocument !== $mine) {
            throw new DOMException('Wrong Document Error', 4);
        }
        if ($old !== null) {
            $old->ownerElement = null;
            $old->parentNode = null;
        }
        $attr->ownerElement = $this;
        $attr->parentNode = $this;
        $attr->ownerDocument = $mine;
        $this->attrNodes[$attr->nodeName] = $attr;
        $this->syncAttrCount();
        return $old;
    }

    public function removeAttribute(string $qualifiedName): bool
    {
        $key = $this->lookupAttrKey($qualifiedName);
        if (!isset($this->attrNodes[$key])) {
            return false;
        }
        $attr = $this->attrNodes[$key];
        $attr->ownerElement = null;
        $attr->parentNode = null;
        // Rebuilt rather than unset(): unsetting an object entry leaves the object unreachable but uncollected.
        $kept = [];
        foreach ($this->attrNodes as $name => $node) {
            if ($name !== $key) {
                $kept[$name] = $node;
            }
        }
        $this->attrNodes = $kept;
        $this->syncAttrCount();
        return true;
    }

    public function removeAttributeNode(DOMAttr $attr): DOMAttr|false
    {
        if ($attr->ownerElement !== $this) {
            throw new DOMException('Not Found Error', 8);
        }
        $this->removeAttribute($attr->nodeName);
        return $attr;
    }

    public function hasAttribute(string $qualifiedName): bool
    {
        $key = $this->lookupAttrKey($qualifiedName);
        if (isset($this->attrNodes[$key])) {
            return true;
        }
        if (str_starts_with($qualifiedName, 'xmlns')) {
            $declared = $qualifiedName === 'xmlns' ? '' : substr($qualifiedName, 6);
            return isset($this->nsDecl[$declared]);
        }
        return false;
    }

    public function getAttributeNS(?string $namespace, string $localName): string
    {
        $attr = $this->getAttributeNodeNS($namespace, $localName);
        return $attr === null ? '' : $attr->value;
    }

    public function getAttributeNodeNS(?string $namespace, string $localName): ?DOMAttr
    {
        $wanted = $namespace === '' ? null : $namespace;
        foreach ($this->attrNodes as $attr) {
            if ($attr->localName === $localName && $attr->namespaceURI === $wanted) {
                return $attr;
            }
        }
        return null;
    }

    public function hasAttributeNS(?string $namespace, string $localName): bool
    {
        return $this->getAttributeNodeNS($namespace, $localName) !== null;
    }

    public function setAttributeNS(?string $namespace, string $qualifiedName, mixed $value): void
    {
        $value = (string) $value;
        $colon = strpos($qualifiedName, ':');
        $prefix = $colon === false ? '' : substr($qualifiedName, 0, $colon);
        if ($namespace === 'http://www.w3.org/2000/xmlns/' || $prefix === 'xmlns' || $qualifiedName === 'xmlns') {
            $declared = $prefix;
            if ($qualifiedName === 'xmlns') {
                $declared = '';
            } elseif ($prefix === 'xmlns') {
                $declared = substr($qualifiedName, 6);
            }
            $this->nsDecl[$declared] = $value;
            return;
        }
        $attr = $this->setAttribute($qualifiedName, $value);
        if ($namespace !== null && $namespace !== '') {
            $attr->applyNamespace($namespace);
            if ($prefix !== '' && !isset($this->nsDecl[$prefix])) {
                $this->nsDecl[$prefix] = $namespace;
                $this->nsAuto[$prefix] = true;
            }
        }
    }

    public function removeAttributeNS(?string $namespace, string $localName): void
    {
        $attr = $this->getAttributeNodeNS($namespace, $localName);
        if ($attr !== null) {
            $this->removeAttribute($attr->nodeName);
        }
    }

    public function setIdAttribute(string $qualifiedName, bool $isId): void
    {
        $_unused = [$qualifiedName, $isId];
    }

    public function getElementsByTagName(string $qualifiedName): DOMNodeList
    {
        $found = [];
        __ElephcDom::collectByTag($this, $qualifiedName, $found);
        return new DOMNodeList(null, $found);
    }

    public function getElementsByTagNameNS(?string $namespace, string $localName): DOMNodeList
    {
        $found = [];
        __ElephcDom::collectByTagNS($this, $namespace, $localName, $found);
        return new DOMNodeList(null, $found);
    }

    public function remove(): void
    {
        $this->detach();
    }

    public function append(DOMNode|string ...$nodes): void
    {
        foreach ($nodes as $node) {
            $this->appendChild($node instanceof DOMNode ? $node : new DOMText($node));
        }
    }

    public function prepend(DOMNode|string ...$nodes): void
    {
        $first = $this->firstChild;
        foreach ($nodes as $node) {
            $this->insertBefore($node instanceof DOMNode ? $node : new DOMText($node), $first);
        }
    }

    public function before(DOMNode|string ...$nodes): void
    {
        if ($this->parentNode === null) {
            return;
        }
        foreach ($nodes as $node) {
            $this->parentNode->insertBefore($node instanceof DOMNode ? $node : new DOMText($node), $this);
        }
    }

    public function after(DOMNode|string ...$nodes): void
    {
        if ($this->parentNode === null) {
            return;
        }
        $anchor = $this->nextSibling;
        foreach ($nodes as $node) {
            $this->parentNode->insertBefore($node instanceof DOMNode ? $node : new DOMText($node), $anchor);
        }
    }

    public function replaceWith(DOMNode|string ...$nodes): void
    {
        if ($this->parentNode === null) {
            return;
        }
        $anchor = $this->nextSibling;
        $parent = $this->parentNode;
        $this->detach();
        foreach ($nodes as $node) {
            $parent->insertBefore($node instanceof DOMNode ? $node : new DOMText($node), $anchor);
        }
    }

    public function replaceChildren(DOMNode|string ...$nodes): void
    {
        while ($this->firstChild !== null) {
            $this->firstChild->detach();
        }
        foreach ($nodes as $node) {
            $this->appendChild($node instanceof DOMNode ? $node : new DOMText($node));
        }
    }

    public function shallowCopy(): DOMNode
    {
        $copy = new DOMElement($this->nodeName);
        $copy->namespaceURI = $this->namespaceURI;
        $copy->prefix = $this->prefix;
        $copy->localName = $this->localName;
        $copy->nsDecl = $this->nsDecl;
        $copy->nsAuto = $this->nsAuto;
        foreach ($this->attrNodes as $key => $attr) {
            $clone = $attr->shallowCopy();
            $clone->ownerElement = $copy;
            $clone->parentNode = $copy;
            $copy->attrNodes[$key] = $clone;
        }
        $copy->syncAttrCount();
        return $copy;
    }
}

class DOMDocumentFragment extends DOMNode
{
    public function __construct()
    {
        parent::__construct();
        $this->nodeType = 11;
        $this->nodeName = '#document-fragment';
    }

    public function appendXML(string $data): bool
    {
        $doc = new DOMDocument();
        if (!$doc->loadXML('<elephc-fragment>' . $data . '</elephc-fragment>')) {
            return false;
        }
        $owner = $this->owningDocument();
        $holder = $doc->documentElement;
        while ($holder !== null && $holder->firstChild !== null) {
            $moved = $holder->firstChild;
            $moved->detach();
            $moved->setOwner($owner);
            $this->linkBefore($moved, null);
        }
        return true;
    }

    public function shallowCopy(): DOMNode
    {
        return new DOMDocumentFragment();
    }
}

class DOMDocumentType extends DOMNode
{
    public string $publicId = '';
    public string $systemId = '';
    public ?string $internalSubset = null;

    public function __construct(string $name = '', string $publicId = '', string $systemId = '')
    {
        parent::__construct();
        $this->nodeType = 10;
        $this->nodeName = $name;
        $this->name = $name;
        $this->publicId = $publicId;
        $this->systemId = $systemId;
    }

    public function shallowCopy(): DOMNode
    {
        $copy = new DOMDocumentType($this->name, $this->publicId, $this->systemId);
        $copy->internalSubset = $this->internalSubset;
        return $copy;
    }
}

class DOMDocument extends DOMNode
{
    public ?DOMDocumentType $doctype = null;
    public ?DOMElement $documentElement = null;
    public ?string $encoding = null;
    public ?string $xmlEncoding = null;
    public bool $standalone = false;
    public bool $xmlStandalone = false;
    public ?string $version = '1.0';
    public ?string $xmlVersion = '1.0';
    public bool $strictErrorChecking = true;
    public ?string $documentURI = null;
    public bool $formatOutput = false;
    public bool $validateOnParse = false;
    public bool $resolveExternals = false;
    public bool $preserveWhiteSpace = true;
    public bool $recover = false;
    public bool $substituteEntities = false;
    public bool $isHtml = false;
    public bool $building = false;
    public bool $standaloneSet = false;
    public array $idIndex = [];

    public function __construct(string $version = '1.0', string $encoding = '')
    {
        parent::__construct();
        // A node tree is a web of parent / child / sibling links, which reference counting alone never
        // frees; the cycle collector only runs on request, so reclaim dropped documents as new ones appear.
        __ElephcDomState::$documents = __ElephcDomState::$documents + 1;
        if (__ElephcDomState::$documents % 8 === 0) {
            gc_collect_cycles();
        }
        $this->nodeType = 9;
        $this->nodeName = '#document';
        $this->version = $version;
        $this->xmlVersion = $version;
        if ($encoding !== '') {
            $this->encoding = $encoding;
        }
    }

    public function childrenChanged(): void
    {
        $this->documentElement = null;
        $this->doctype = null;
        $node = $this->firstChild;
        while ($node !== null) {
            if ($node->nodeType === 1 && $this->documentElement === null) {
                $this->documentElement = $node;
            } elseif ($node->nodeType === 10 && $this->doctype === null) {
                $this->doctype = $node;
            }
            $node = $node->nextSibling;
        }
    }

    public function createElement(string $localName, mixed $value = ''): DOMElement
    {
        $value = (string) $value;
        $name = $localName;
        $element = new DOMElement($name);
        $element->ownerDocument = $this;
        if ($value !== '') {
            $text = __ElephcDom::decodeValue($value);
            if ($text === null) {
                return $element;
            }
            if ($text !== '') {
                $node = new DOMText($text);
                $node->ownerDocument = $this;
                $element->linkBefore($node, null);
            }
        }
        return $element;
    }

    public function createElementNS(?string $namespace, string $qualifiedName, mixed $value = ''): DOMElement
    {
        $value = (string) $value;
        $element = new DOMElement($qualifiedName, null, $namespace === null ? '' : $namespace);
        $element->ownerDocument = $this;
        if ($value !== '') {
            $text = __ElephcDom::decodeValue($value);
            if ($text !== null && $text !== '') {
                $node = new DOMText($text);
                $node->ownerDocument = $this;
                $element->linkBefore($node, null);
            }
        }
        return $element;
    }

    public function createTextNode(mixed $data): DOMText
    {
        $data = (string) $data;
        $text = new DOMText($data);
        $text->ownerDocument = $this;
        return $text;
    }

    public function createComment(mixed $data): DOMComment
    {
        $data = (string) $data;
        $comment = new DOMComment($data);
        $comment->ownerDocument = $this;
        return $comment;
    }

    public function createCDATASection(mixed $data): DOMCdataSection
    {
        $data = (string) $data;
        $cdata = new DOMCdataSection($data);
        $cdata->ownerDocument = $this;
        return $cdata;
    }

    public function createDocumentFragment(): DOMDocumentFragment
    {
        $fragment = new DOMDocumentFragment();
        $fragment->ownerDocument = $this;
        return $fragment;
    }

    public function createAttribute(string $localName): DOMAttr
    {
        if (!__ElephcDom::validName($localName)) {
            throw new DOMException('Invalid Character Error', 5);
        }
        $attr = new DOMAttr($localName);
        $attr->ownerDocument = $this;
        return $attr;
    }

    public function createAttributeNS(?string $namespace, string $qualifiedName): DOMAttr
    {
        $attr = $this->createAttribute($qualifiedName);
        if ($namespace !== null && $namespace !== '') {
            $attr->applyNamespace($namespace);
        }
        return $attr;
    }

    public function createProcessingInstruction(string $target, mixed $data = ''): DOMProcessingInstruction
    {
        $data = (string) $data;
        if (!__ElephcDom::validName($target)) {
            throw new DOMException('Invalid Character Error', 5);
        }
        $pi = new DOMProcessingInstruction($target, $data);
        $pi->ownerDocument = $this;
        return $pi;
    }

    public function importNode(DOMNode $node, bool $deep = false): DOMNode
    {
        if ($node->nodeType === 9 || $node->nodeType === 13) {
            throw new DOMException('Not Supported Error', 9);
        }
        if ($node->owningDocument() === $this) {
            return $node;
        }
        $copy = $node->cloneNode($deep);
        $copy->setOwner($this);
        return $copy;
    }

    public function adoptNode(DOMNode $node): DOMNode
    {
        if ($node->nodeType === 9 || $node->nodeType === 13 || $node->nodeType === 10) {
            throw new DOMException('Not Supported Error', 9);
        }
        if ($node instanceof DOMAttr && $node->ownerElement !== null) {
            $node->ownerElement->removeAttribute($node->nodeName);
        }
        $node->detach();
        $node->setOwner($this);
        return $node;
    }

    public function getElementsByTagName(string $qualifiedName): DOMNodeList
    {
        $found = [];
        __ElephcDom::collectByTag($this, $qualifiedName, $found);
        return new DOMNodeList(null, $found);
    }

    public function getElementsByTagNameNS(?string $namespace, string $localName): DOMNodeList
    {
        $found = [];
        __ElephcDom::collectByTagNS($this, $namespace, $localName, $found);
        return new DOMNodeList(null, $found);
    }

    public function getElementById(string $elementId): ?DOMElement
    {
        return __ElephcDom::findById($this, $elementId, $this->isHtml);
    }

    public function normalizeDocument(): void
    {
        $this->normalize();
    }

    public function saveXML(?DOMNode $node = null, int $options = 0): string|false
    {
        $this->syncText();
        if ($node === null || $node === $this) {
            return __ElephcDomWriter::document($this, $this->formatOutput, $options);
        }
        if ($node->owningDocument() !== $this) {
            throw new DOMException('Wrong Document Error', 4);
        }
        return __ElephcDomWriter::xml($node, $this->formatOutput, $this->encoding === null, $options);
    }

    public function saveHTML(?DOMNode $node = null): string|false
    {
        $this->syncText();
        if ($node === null || $node === $this) {
            return __ElephcDomHtmlWriter::document($this, $this->formatOutput);
        }
        if ($node->owningDocument() !== $this) {
            throw new DOMException('Wrong Document Error', 4);
        }
        return __ElephcDomHtmlWriter::dumpNode($this, $node, $this->formatOutput);
    }

    public function save(string $filename, int $options = 0): int|false
    {
        $xml = $this->saveXML(null, $options);
        if ($xml === false) {
            return false;
        }
        return file_put_contents($filename, $xml);
    }

    public function saveHTMLFile(string $filename): int|false
    {
        $html = $this->saveHTML();
        if ($html === false) {
            return false;
        }
        return file_put_contents($filename, $html);
    }

    public function load(string $filename, int $options = 0): bool
    {
        if (!is_readable($filename)) {
            return false;
        }
        $source = file_get_contents($filename);
        if ($source === false) {
            return false;
        }
        return $this->loadXML($source, $options);
    }

    public function loadHTMLFile(string $filename, int $options = 0): bool
    {
        if (!is_readable($filename)) {
            return false;
        }
        $source = file_get_contents($filename);
        if ($source === false) {
            return false;
        }
        return $this->loadHTML($source, $options);
    }

    public function loadXML(string $source, int $options = 0): bool
    {
        if ($source === '') {
            throw new \ValueError('DOMDocument::loadXML(): Argument #1 ($source) must not be empty');
        }
        $this->clearForLoad();
        $parser = new __ElephcXmlParser($this, $source, $options);
        $this->building = true;
        $parsed = $parser->parse();
        $this->building = false;
        if (!$parsed) {
            $this->clearForLoad();
            return false;
        }
        __ElephcDom::rebuildText($this);
        return true;
    }

    public function loadHTML(string $source, int $options = 0): bool
    {
        if ($source === '') {
            throw new \ValueError('DOMDocument::loadHTML(): Argument #1 ($source) must not be empty');
        }
        $this->clearForLoad();
        $parser = new __ElephcHtmlParser($this, $source, $options);
        $this->building = true;
        $parser->parse();
        $this->building = false;
        __ElephcDom::rebuildText($this);
        return true;
    }

    public function clearForLoad(): void
    {
        $this->building = true;
        while ($this->firstChild !== null) {
            $this->firstChild->detach();
        }
        $this->textContent = '';
        $this->textCache = '';
        $this->building = false;
        $this->idIndex = [];
        $this->isHtml = false;
        $this->nodeType = 9;
        $this->nodeName = '#document';
    }

    public function schemaValidate(string $filename, int $flags = 0): bool
    {
        $_unused = [$filename, $flags];
        return false;
    }

    public function schemaValidateSource(string $source, int $flags = 0): bool
    {
        $_unused = [$source, $flags];
        return false;
    }

    public function relaxNGValidate(string $filename): bool
    {
        $_unused = $filename;
        return false;
    }

    public function relaxNGValidateSource(string $source): bool
    {
        $_unused = $source;
        return false;
    }

    public function validate(): bool
    {
        return false;
    }

    public function xinclude(int $options = 0): int|false
    {
        $_unused = $options;
        return 0;
    }

    public function shallowCopy(): DOMNode
    {
        $copy = new DOMDocument($this->version === null ? '1.0' : $this->version, $this->encoding === null ? '' : $this->encoding);
        $copy->isHtml = $this->isHtml;
        $copy->nodeType = $this->nodeType;
        $copy->formatOutput = $this->formatOutput;
        return $copy;
    }

    public function cloneNode(bool $deep = false): DOMNode
    {
        $copy = $this->shallowCopy();
        if ($deep) {
            $child = $this->firstChild;
            while ($child !== null) {
                $clone = $child->cloneNode(true);
                $clone->setOwner($copy);
                $copy->linkBefore($clone, null);
                $child = $child->nextSibling;
            }
        }
        return $copy;
    }
}

final class __ElephcDom
{
    // Fills every node's cached text once, bottom-up, after a parser built the tree without refreshing.
    public static function rebuildText(DOMNode $node): void
    {
        $child = $node->firstChild;
        while ($child !== null) {
            if ($child->nodeType === 1) {
                self::rebuildText($child);
            }
            $child = $child->nextSibling;
        }
        $node->computeText();
    }

    public static function validName(string $name): bool
    {
        $length = strlen($name);
        if ($length === 0) {
            return false;
        }
        for ($i = 0; $i < $length; $i++) {
            $code = ord($name[$i]);
            $letter = ($code >= 65 && $code <= 90) || ($code >= 97 && $code <= 122) || $code === 95 || $code === 58 || $code >= 128;
            if ($i === 0) {
                if (!$letter) {
                    return false;
                }
            } elseif (!$letter && !($code >= 48 && $code <= 57) && $code !== 45 && $code !== 46) {
                return false;
            }
        }
        return true;
    }

    public static function charLength(string $text): int
    {
        $length = strlen($text);
        $count = 0;
        for ($i = 0; $i < $length; $i++) {
            if ((ord($text[$i]) & 0xC0) !== 0x80) {
                $count++;
            }
        }
        return $count;
    }

    public static function charSlice(string $text, int $offset, int $count): string
    {
        $length = strlen($text);
        $chars = 0;
        $start = $length;
        $end = $length;
        for ($i = 0; $i < $length; $i++) {
            if ((ord($text[$i]) & 0xC0) !== 0x80) {
                if ($chars === $offset) {
                    $start = $i;
                }
                if ($chars === $offset + $count) {
                    $end = $i;
                    break;
                }
                $chars++;
            }
        }
        if ($start > $end) {
            return '';
        }
        return substr($text, $start, $end - $start);
    }

    // createElement()'s value is parsed for entity references, so a bare ampersand fails.
    public static function decodeValue(string $value): ?string
    {
        if (strpos($value, '&') === false) {
            return $value;
        }
        $out = '';
        $length = strlen($value);
        $i = 0;
        while ($i < $length) {
            $char = $value[$i];
            if ($char !== '&') {
                $out .= $char;
                $i++;
                continue;
            }
            $semi = strpos($value, ';', $i);
            if ($semi === false) {
                return null;
            }
            $decoded = self::decodeReference(substr($value, $i + 1, $semi - $i - 1));
            if ($decoded === null) {
                return null;
            }
            $out .= $decoded;
            $i = $semi + 1;
        }
        return $out;
    }

    public static function decodeReference(string $name): ?string
    {
        if ($name === 'amp') {
            return '&';
        }
        if ($name === 'lt') {
            return '<';
        }
        if ($name === 'gt') {
            return '>';
        }
        if ($name === 'quot') {
            return '"';
        }
        if ($name === 'apos') {
            return "'";
        }
        if (strlen($name) > 1 && $name[0] === '#') {
            $number = $name[1] === 'x' ? hexdec(substr($name, 2)) : (int) substr($name, 1);
            if ($number <= 0 || $number > 0x10FFFF) {
                return null;
            }
            return self::utf8($number);
        }
        return null;
    }

    public static function utf8(int $code): string
    {
        if ($code < 0x80) {
            return chr($code);
        }
        if ($code < 0x800) {
            return chr(0xC0 | ($code >> 6)) . chr(0x80 | ($code & 0x3F));
        }
        if ($code < 0x10000) {
            return chr(0xE0 | ($code >> 12)) . chr(0x80 | (($code >> 6) & 0x3F)) . chr(0x80 | ($code & 0x3F));
        }
        return chr(0xF0 | ($code >> 18)) . chr(0x80 | (($code >> 12) & 0x3F)) . chr(0x80 | (($code >> 6) & 0x3F)) . chr(0x80 | ($code & 0x3F));
    }

    public static function collectByTag(DOMNode $root, string $name, array &$found): void
    {
        $wanted = $name;
        $node = $root->firstChild;
        while ($node !== null) {
            if ($node->nodeType === 1) {
                if ($wanted === '*' || $node->nodeName === $wanted) {
                    $found[] = $node;
                }
                self::collectByTag($node, $name, $found);
            }
            $node = $node->nextSibling;
        }
    }

    public static function collectByTagNS(DOMNode $root, ?string $namespace, string $localName, array &$found): void
    {
        $node = $root->firstChild;
        while ($node !== null) {
            if ($node->nodeType === 1) {
                $nsMatch = $namespace === '*' || $node->namespaceURI === ($namespace === '' ? null : $namespace);
                if ($nsMatch && ($localName === '*' || $node->localName === $localName)) {
                    $found[] = $node;
                }
                self::collectByTagNS($node, $namespace, $localName, $found);
            }
            $node = $node->nextSibling;
        }
    }

    public static function findById(DOMNode $root, string $id, bool $html): ?DOMElement
    {
        $node = $root->firstChild;
        while ($node !== null) {
            if ($node instanceof DOMElement) {
                if ($html && isset($node->attrNodes['id']) && $node->attrNodes['id']->value === $id) {
                    return $node;
                }
                $inner = self::findById($node, $id, $html);
                if ($inner !== null) {
                    return $inner;
                }
            }
            $node = $node->nextSibling;
        }
        return null;
    }

    public static function escapeText(string $text, bool $ascii): string
    {
        $text = str_replace(['&', '<', '>', "\r"], ['&amp;', '&lt;', '&gt;', $ascii ? '&#xD;' : '&#13;'], $text);
        return $ascii ? self::asciiRefs($text) : $text;
    }

    public static function escapeAttr(string $text, bool $ascii): string
    {
        $text = str_replace(['&', '<', '>', '"', "\n", "\r", "\t"], ['&amp;', '&lt;', '&gt;', '&quot;', '&#10;', '&#13;', '&#9;'], $text);
        return $ascii ? self::asciiRefs($text) : $text;
    }

    public static function asciiRefs(string $text): string
    {
        $length = strlen($text);
        $clean = true;
        for ($i = 0; $i < $length; $i++) {
            if (ord($text[$i]) >= 128) {
                $clean = false;
                break;
            }
        }
        if ($clean) {
            return $text;
        }
        $out = '';
        $i = 0;
        while ($i < $length) {
            $code = ord($text[$i]);
            if ($code < 128) {
                $out .= $text[$i];
                $i++;
            } elseif ($code >= 0xF0 && $i + 3 < $length) {
                $point = (($code & 0x07) << 18) | ((ord($text[$i + 1]) & 0x3F) << 12) | ((ord($text[$i + 2]) & 0x3F) << 6) | (ord($text[$i + 3]) & 0x3F);
                $out .= '&#x' . strtoupper(dechex($point)) . ';';
                $i += 4;
            } elseif ($code >= 0xE0 && $i + 2 < $length) {
                $point = (($code & 0x0F) << 12) | ((ord($text[$i + 1]) & 0x3F) << 6) | (ord($text[$i + 2]) & 0x3F);
                $out .= '&#x' . strtoupper(dechex($point)) . ';';
                $i += 3;
            } elseif ($code >= 0xC0 && $i + 1 < $length) {
                $point = (($code & 0x1F) << 6) | (ord($text[$i + 1]) & 0x3F);
                $out .= '&#x' . strtoupper(dechex($point)) . ';';
                $i += 2;
            } else {
                $out .= '&#x' . strtoupper(dechex($code)) . ';';
                $i++;
            }
        }
        return $out;
    }
}

final class __ElephcDomWriter
{
    public string $out = '';
    public int $level = 0;
    public bool $fmt = false;
    public bool $ascii = false;
    public int $options = 0;
    public bool $xhtml = false;
    public bool $rawText = false;

    public static function isXhtml(?DOMDocumentType $dtd): bool
    {
        if ($dtd === null) {
            return false;
        }
        $ids = ['-//W3C//DTD XHTML 1.0 Strict//EN', '-//W3C//DTD XHTML 1.0 Frameset//EN', '-//W3C//DTD XHTML 1.0 Transitional//EN', 'http://www.w3.org/TR/xhtml1/DTD/xhtml1-strict.dtd', 'http://www.w3.org/TR/xhtml1/DTD/xhtml1-frameset.dtd', 'http://www.w3.org/TR/xhtml1/DTD/xhtml1-transitional.dtd'];
        return in_array($dtd->publicId, $ids, true) || in_array($dtd->systemId, $ids, true);
    }

    public static function header(DOMDocument $doc): string
    {
        $head = '<?xml version="' . ($doc->version === null ? '1.0' : $doc->version) . '"';
        if ($doc->encoding !== null) {
            $head .= ' encoding="' . $doc->encoding . '"';
        }
        if ($doc->standaloneSet) {
            $head .= $doc->xmlStandalone ? ' standalone="yes"' : ' standalone="no"';
        }
        return $head . "?>\n";
    }

    public static function document(DOMDocument $doc, bool $format, int $options): string
    {
        $writer = new __ElephcDomWriter();
        $writer->fmt = $format;
        $writer->ascii = $doc->encoding === null;
        $writer->options = $options;
        $dtd = $doc->doctype;
        $writer->xhtml = self::isXhtml($dtd);
        $out = ($options & 2) !== 0 ? '' : self::header($doc);
        $child = $doc->firstChild;
        while ($child !== null) {
            $writer->out = '';
            $writer->level = 0;
            $writer->fmt = $format;
            $writer->node($child, true, $format, []);
            $out .= $writer->out . "\n";
            $child = $child->nextSibling;
        }
        return $out;
    }

    public static function xml(DOMNode $node, bool $format, bool $ascii, int $options): string
    {
        $writer = new __ElephcDomWriter();
        $writer->fmt = $format;
        $writer->ascii = $ascii;
        $writer->options = $options;
        $owner = $node->owningDocument();
        $ownerDtd = $owner === null ? null : $owner->doctype;
        $writer->xhtml = $owner !== null && self::isXhtml($ownerDtd);
        $writer->rawText = true;
        if ($node instanceof DOMDocument) {
            return self::document($node, $format, $options);
        }
        $writer->node($node, true, $format, []);
        return $writer->out;
    }

    public function indent(): void
    {
        if ($this->fmt) {
            $this->out .= str_repeat('  ', $this->level);
        }
    }

    public function node(DOMNode $cur, bool $isRoot, bool $origFormat, array $scope): void
    {
        $type = $cur->nodeType;
        if ($cur instanceof DOMElement) {
            $this->element($cur, $isRoot, $origFormat, $scope);
        } elseif ($cur instanceof DOMCdataSection) {
            $this->out .= '<![CDATA[' . str_replace(']]>', ']]]]><![CDATA[>', $cur->data) . ']]>';
        } elseif ($cur instanceof DOMText) {
            $this->out .= __ElephcDom::escapeText($cur->data, $this->ascii && !$this->rawText);
        } elseif ($cur instanceof DOMComment) {
            if (!$isRoot) {
                $this->indent();
            }
            $this->out .= '<!--' . $cur->data . '-->';
        } elseif ($cur instanceof DOMProcessingInstruction) {
            if (!$isRoot) {
                $this->indent();
            }
            $this->out .= $cur->data === '' ? '<?' . $cur->target . '?>' : '<?' . $cur->target . ' ' . $cur->data . '?>';
        } elseif ($cur instanceof DOMDocumentType) {
            $this->out .= self::doctype($cur);
        } elseif ($type === 11) {
            $child = $cur->firstChild;
            while ($child !== null) {
                $this->node($child, false, $origFormat, $scope);
                $child = $child->nextSibling;
            }
        } elseif ($cur instanceof DOMAttr) {
            $this->out .= __ElephcDom::escapeText($cur->value, $this->ascii);
        }
    }

    public static function doctype(DOMDocumentType $dtd): string
    {
        $out = '<!DOCTYPE ' . $dtd->name;
        if ($dtd->publicId !== '') {
            $out .= ' PUBLIC "' . $dtd->publicId . '" "' . $dtd->systemId . '"';
        } elseif ($dtd->systemId !== '') {
            $out .= ' SYSTEM "' . $dtd->systemId . '"';
        }
        if ($dtd->internalSubset !== null && $dtd->internalSubset !== '') {
            $out .= ' [' . $dtd->internalSubset . ']';
        }
        return $out . '>';
    }

    // True when libxml would stop indenting inside this element: it has a text, CDATA or entity child.
    public static function hasTextChild(DOMNode $cur): bool
    {
        $child = $cur->firstChild;
        while ($child !== null) {
            $kind = $child->nodeType;
            if ($kind === 3 || $kind === 4 || $kind === 5) {
                return true;
            }
            $child = $child->nextSibling;
        }
        return false;
    }

    public function element(DOMElement $cur, bool $isRoot, bool $origFormat, array $scope): void
    {
        if (!$isRoot) {
            $this->indent();
        }
        $this->out .= '<' . $cur->nodeName;
        if ($this->xhtml && $cur->nodeName === 'html' && $cur->namespaceURI === null && !isset($cur->nsDecl[''])) {
            $this->out .= ' xmlns="http://www.w3.org/1999/xhtml"';
        }
        foreach ($cur->nsDecl as $declared => $uri) {
            if (isset($cur->nsAuto[$declared]) && ($scope[(string) $declared] ?? null) === $uri) {
                continue;
            }
            $this->out .= $declared === '' ? ' xmlns="' . __ElephcDom::escapeAttr($uri, $this->ascii) . '"' : ' xmlns:' . $declared . '="' . __ElephcDom::escapeAttr($uri, $this->ascii) . '"';
            $scope[(string) $declared] = $uri;
        }
        if ($cur->namespaceURI !== null && ($scope[$cur->prefix] ?? null) !== $cur->namespaceURI) {
            $this->out .= $cur->prefix === '' ? ' xmlns="' . __ElephcDom::escapeAttr($cur->namespaceURI, $this->ascii) . '"' : ' xmlns:' . $cur->prefix . '="' . __ElephcDom::escapeAttr($cur->namespaceURI, $this->ascii) . '"';
            $scope[$cur->prefix] = $cur->namespaceURI;
        }
        foreach ($cur->attrNodes as $attr) {
            if ($attr->namespaceURI !== null && $attr->prefix !== '' && $attr->prefix !== 'xml' && ($scope[$attr->prefix] ?? null) !== $attr->namespaceURI) {
                $this->out .= ' xmlns:' . $attr->prefix . '="' . __ElephcDom::escapeAttr($attr->namespaceURI, $this->ascii) . '"';
                $scope[$attr->prefix] = $attr->namespaceURI;
            }
            $this->out .= ' ' . $attr->nodeName . '="' . __ElephcDom::escapeAttr($attr->value, $this->ascii) . '"';
        }
        if ($cur->firstChild === null) {
            if ($this->xhtml) {
                $this->out .= $cur->namespaceURI === null && in_array($cur->nodeName, ['area', 'base', 'basefont', 'br', 'col', 'frame', 'hr', 'img', 'input', 'isindex', 'link', 'meta', 'param'], true) ? ' />' : '></' . $cur->nodeName . '>';
            } else {
                $this->out .= ($this->options & 4) !== 0 ? '></' . $cur->nodeName . '>' : '/>';
            }
            return;
        }
        $unformatted = false;
        if ($this->fmt && self::hasTextChild($cur)) {
            $this->fmt = false;
            $unformatted = true;
        }
        $this->out .= '>';
        if ($this->fmt) {
            $this->out .= "\n";
        }
        $this->level++;
        $child = $cur->firstChild;
        while ($child !== null) {
            $this->node($child, false, $origFormat, $scope);
            if ($this->fmt) {
                $this->out .= "\n";
            }
            $child = $child->nextSibling;
        }
        if ($this->level > 0) {
            $this->level--;
        }
        if ($this->fmt) {
            $this->out .= str_repeat('  ', $this->level);
        }
        $this->out .= '</' . $cur->nodeName . '>';
        if ($unformatted) {
            $this->fmt = $origFormat;
        }
    }
}

final class __ElephcXmlParser
{
    public DOMDocument $doc;
    public string $src = '';
    public int $pos = 0;
    public int $len = 0;
    public int $options = 0;
    public array $scopes = [];

    public function __construct(DOMDocument $doc, string $src, int $options)
    {
        $this->doc = $doc;
        $this->src = $src;
        $this->len = strlen($src);
        $this->options = $options;
    }

    public function startsWith(string $token): bool
    {
        return substr($this->src, $this->pos, strlen($token)) === $token;
    }

    public function skipSpace(): void
    {
        while ($this->pos < $this->len) {
            $c = $this->src[$this->pos];
            if ($c === ' ' || $c === "\t" || $c === "\n" || $c === "\r") {
                $this->pos++;
            } else {
                break;
            }
        }
    }

    public function parse(): bool
    {
        if (substr($this->src, 0, 3) === "\xEF\xBB\xBF") {
            $this->pos = 3;
        }
        if ($this->startsWith('<?xml') && $this->pos + 5 < $this->len && strpos(" \t\r\n", $this->src[$this->pos + 5]) !== false) {
            if (!$this->parseDeclaration()) {
                return false;
            }
        }
        $sawRoot = false;
        while (true) {
            $this->skipSpace();
            if ($this->pos >= $this->len) {
                break;
            }
            if ($this->startsWith('<!--')) {
                $comment = $this->parseComment();
                if ($comment === null) {
                    return false;
                }
                $this->doc->linkBefore($comment, null);
            } elseif ($this->startsWith('<?')) {
                $pi = $this->parseProcessingInstruction();
                if ($pi === null) {
                    return false;
                }
                $this->doc->linkBefore($pi, null);
            } elseif ($this->startsWith('<!DOCTYPE')) {
                if ($sawRoot || $this->doc->doctype !== null) {
                    return false;
                }
                $dtd = $this->parseDoctype();
                if ($dtd === null) {
                    return false;
                }
                $this->doc->linkBefore($dtd, null);
            } elseif ($this->src[$this->pos] === '<') {
                if ($sawRoot) {
                    return false;
                }
                $root = $this->parseElement();
                if ($root === null) {
                    return false;
                }
                $this->doc->linkBefore($root, null);
                $sawRoot = true;
            } else {
                return false;
            }
        }
        return $sawRoot;
    }

    public function parseDeclaration(): bool
    {
        $end = strpos($this->src, '?>', $this->pos);
        if ($end === false) {
            return false;
        }
        $body = substr($this->src, $this->pos + 5, $end - $this->pos - 5);
        $this->pos = $end + 2;
        $version = $this->pseudoAttribute($body, 'version');
        if ($version === null) {
            return false;
        }
        $doc = $this->doc;
        $doc->version = $version;
        $doc->xmlVersion = $version;
        $encoding = $this->pseudoAttribute($body, 'encoding');
        if ($encoding !== null) {
            $doc->encoding = $encoding;
            $doc->xmlEncoding = $encoding;
        }
        $standalone = $this->pseudoAttribute($body, 'standalone');
        if ($standalone !== null) {
            $doc->standaloneSet = true;
            $doc->xmlStandalone = $standalone === 'yes';
            $doc->standalone = $standalone === 'yes';
        }
        return true;
    }

    public function pseudoAttribute(string $body, string $name): ?string
    {
        $at = strpos($body, $name);
        if ($at === false) {
            return null;
        }
        $rest = ltrim(substr($body, $at + strlen($name)), " \t\r\n");
        if ($rest === '' || $rest[0] !== '=') {
            return null;
        }
        $rest = ltrim(substr($rest, 1), " \t\r\n");
        if ($rest === '' || ($rest[0] !== '"' && $rest[0] !== "'")) {
            return null;
        }
        $close = strpos($rest, $rest[0], 1);
        if ($close === false) {
            return null;
        }
        return substr($rest, 1, $close - 1);
    }

    public function parseComment(): ?DOMComment
    {
        $end = strpos($this->src, '-->', $this->pos + 4);
        if ($end === false) {
            return null;
        }
        $comment = new DOMComment(substr($this->src, $this->pos + 4, $end - $this->pos - 4));
        $comment->ownerDocument = $this->doc;
        $this->pos = $end + 3;
        return $comment;
    }

    public function parseProcessingInstruction(): ?DOMProcessingInstruction
    {
        $end = strpos($this->src, '?>', $this->pos + 2);
        if ($end === false) {
            return null;
        }
        $body = substr($this->src, $this->pos + 2, $end - $this->pos - 2);
        $this->pos = $end + 2;
        $space = strcspn($body, " \t\r\n");
        $target = substr($body, 0, $space);
        if (!__ElephcDom::validName($target) || strtolower($target) === 'xml') {
            return null;
        }
        $pi = new DOMProcessingInstruction($target, ltrim(substr($body, $space), " \t\r\n"));
        $pi->ownerDocument = $this->doc;
        return $pi;
    }

    public function parseDoctype(): ?DOMDocumentType
    {
        $this->pos += 9;
        $this->skipSpace();
        $start = $this->pos;
        $this->pos += strcspn($this->src, " \t\r\n>[", $this->pos);
        $name = substr($this->src, $start, $this->pos - $start);
        $public = '';
        $system = '';
        $this->skipSpace();
        if ($this->startsWith('PUBLIC')) {
            $this->pos += 6;
            $this->skipSpace();
            $public = $this->quoted();
            $this->skipSpace();
            if ($this->pos < $this->len && ($this->src[$this->pos] === '"' || $this->src[$this->pos] === "'")) {
                $system = $this->quoted();
            }
        } elseif ($this->startsWith('SYSTEM')) {
            $this->pos += 6;
            $this->skipSpace();
            $system = $this->quoted();
        }
        $this->skipSpace();
        $subset = null;
        if ($this->pos < $this->len && $this->src[$this->pos] === '[') {
            $close = strpos($this->src, ']', $this->pos);
            if ($close === false) {
                return null;
            }
            $subset = substr($this->src, $this->pos + 1, $close - $this->pos - 1);
            $this->pos = $close + 1;
            $this->skipSpace();
        }
        if ($this->pos >= $this->len || $this->src[$this->pos] !== '>') {
            return null;
        }
        $this->pos++;
        $dtd = new DOMDocumentType($name, $public, $system);
        $dtd->internalSubset = $subset;
        $dtd->ownerDocument = $this->doc;
        return $dtd;
    }

    public function quoted(): string
    {
        if ($this->pos >= $this->len) {
            return '';
        }
        $quote = $this->src[$this->pos];
        $close = strpos($this->src, $quote, $this->pos + 1);
        if ($close === false) {
            $this->pos = $this->len;
            return '';
        }
        $value = substr($this->src, $this->pos + 1, $close - $this->pos - 1);
        $this->pos = $close + 1;
        return $value;
    }

    public function parseName(): string
    {
        $start = $this->pos;
        while ($this->pos < $this->len) {
            $c = $this->src[$this->pos];
            if (strpos(" \t\r\n/>=<'\"", $c) !== false) {
                break;
            }
            $this->pos++;
        }
        return substr($this->src, $start, $this->pos - $start);
    }

    public function lookupNamespace(string $prefix): ?string
    {
        if ($prefix === 'xml') {
            return 'http://www.w3.org/XML/1998/namespace';
        }
        for ($i = count($this->scopes) - 1; $i >= 0; $i--) {
            if (isset($this->scopes[$i][$prefix])) {
                return $this->scopes[$i][$prefix];
            }
        }
        return null;
    }

    public function parseElement(): ?DOMElement
    {
        $this->pos++;
        $name = $this->parseName();
        if (!__ElephcDom::validName($name)) {
            return null;
        }
        $rawAttrs = [];
        $declared = [];
        while (true) {
            $this->skipSpace();
            if ($this->pos >= $this->len) {
                return null;
            }
            $c = $this->src[$this->pos];
            if ($c === '>' || $c === '/') {
                break;
            }
            $attrName = $this->parseName();
            if (!__ElephcDom::validName($attrName)) {
                return null;
            }
            $this->skipSpace();
            if ($this->pos >= $this->len || $this->src[$this->pos] !== '=') {
                return null;
            }
            $this->pos++;
            $this->skipSpace();
            if ($this->pos >= $this->len || ($this->src[$this->pos] !== '"' && $this->src[$this->pos] !== "'")) {
                return null;
            }
            $quote = $this->src[$this->pos];
            $close = strpos($this->src, $quote, $this->pos + 1);
            if ($close === false) {
                return null;
            }
            $raw = substr($this->src, $this->pos + 1, $close - $this->pos - 1);
            if (strpos($raw, '<') !== false) {
                return null;
            }
            $this->pos = $close + 1;
            $value = $this->decodeText($raw, true);
            if ($value === null) {
                return null;
            }
            if (isset($rawAttrs[$attrName]) || isset($declared[$attrName])) {
                return null;
            }
            if ($attrName === 'xmlns') {
                $declared[''] = $value;
                $rawAttrs[$attrName] = null;
            } elseif (str_starts_with($attrName, 'xmlns:')) {
                $declared[substr($attrName, 6)] = $value;
                $rawAttrs[$attrName] = null;
            } else {
                $rawAttrs[$attrName] = $value;
            }
            if ($this->pos < $this->len && strpos(" \t\r\n/>", $this->src[$this->pos]) === false) {
                return null;
            }
        }
        $this->scopes[] = $declared;
        $element = new DOMElement($name);
        $element->ownerDocument = $this->doc;
        $element->nsDecl = $declared;
        $colon = strpos($name, ':');
        if ($colon !== false) {
            $uri = $this->lookupNamespace(substr($name, 0, $colon));
            if ($uri !== null) {
                $element->namespaceURI = $uri;
            } else {
                $element->prefix = '';
                $element->localName = $name;
            }
        } else {
            $uri = $this->lookupNamespace('');
            if ($uri !== null && $uri !== '') {
                $element->namespaceURI = $uri;
            }
        }
        foreach ($rawAttrs as $attrName => $value) {
            if ($value === null) {
                continue;
            }
            $attr = new DOMAttr((string) $attrName, $value);
            $attr->ownerDocument = $this->doc;
            $attr->ownerElement = $element;
            $attr->parentNode = $element;
            $attrColon = strpos((string) $attrName, ':');
            if ($attrColon !== false) {
                $attrUri = $this->lookupNamespace(substr((string) $attrName, 0, $attrColon));
                if ($attrUri !== null) {
                    $attr->applyNamespace($attrUri);
                }
            }
            $element->attrNodes[(string) $attrName] = $attr;
        }
        $element->syncAttrCount();
        if ($this->src[$this->pos] === '/') {
            if (!$this->startsWith('/>')) {
                return null;
            }
            $this->pos += 2;
            array_pop($this->scopes);
            return $element;
        }
        $this->pos++;
        if (!$this->parseContent($element, $name)) {
            return null;
        }
        array_pop($this->scopes);
        return $element;
    }

    public function parseContent(DOMElement $element, string $name): bool
    {
        while (true) {
            if ($this->pos >= $this->len) {
                return false;
            }
            if ($this->src[$this->pos] !== '<') {
                $next = strpos($this->src, '<', $this->pos);
                $stop = $next === false ? $this->len : $next;
                $raw = substr($this->src, $this->pos, $stop - $this->pos);
                $this->pos = $stop;
                $text = $this->decodeText($raw, false);
                if ($text === null) {
                    return false;
                }
                if (($this->options & 256) !== 0 && trim($text, " \t\r\n") === '' && $this->blankIgnorable($element)) {
                    continue;
                }
                $this->appendText($element, $text);
                continue;
            }
            if ($this->startsWith('</')) {
                $this->pos += 2;
                $closing = $this->parseName();
                $this->skipSpace();
                if ($closing !== $name || $this->pos >= $this->len || $this->src[$this->pos] !== '>') {
                    return false;
                }
                $this->pos++;
                return true;
            }
            if ($this->startsWith('<!--')) {
                $comment = $this->parseComment();
                if ($comment === null) {
                    return false;
                }
                $element->linkBefore($comment, null);
            } elseif ($this->startsWith('<![CDATA[')) {
                $end = strpos($this->src, ']]>', $this->pos);
                if ($end === false) {
                    return false;
                }
                $data = substr($this->src, $this->pos + 9, $end - $this->pos - 9);
                $this->pos = $end + 3;
                if (($this->options & 16384) !== 0) {
                    $this->appendText($element, $data);
                } else {
                    $cdata = new DOMCdataSection($data);
                    $cdata->ownerDocument = $this->doc;
                    $element->linkBefore($cdata, null);
                }
            } elseif ($this->startsWith('<?')) {
                $pi = $this->parseProcessingInstruction();
                if ($pi === null) {
                    return false;
                }
                $element->linkBefore($pi, null);
            } else {
                $child = $this->parseElement();
                if ($child === null) {
                    return false;
                }
                $element->linkBefore($child, null);
            }
        }
    }

    // libxml's areBlanks() heuristic: whitespace before a tag is dropped unless it sits in mixed content.
    public function blankIgnorable(DOMElement $element): bool
    {
        if ($this->pos < $this->len && $this->src[$this->pos] !== '<') {
            return false;
        }
        if ($element->firstChild === null && $this->startsWith('</')) {
            return false;
        }
        $last = $element->lastChild;
        if ($last !== null && ($last->nodeType === 3 || $last->nodeType === 4)) {
            return false;
        }
        if ($last !== null && $element->firstChild !== null && ($element->firstChild->nodeType === 3 || $element->firstChild->nodeType === 4)) {
            return false;
        }
        return true;
    }

    public function appendText(DOMElement $element, string $text): void
    {
        $last = $element->lastChild;
        if ($last !== null && $last->nodeType === 3) {
            $last->appendData($text);
            return;
        }
        $node = new DOMText($text);
        $node->ownerDocument = $this->doc;
        $element->linkBefore($node, null);
    }

    // Decodes character and predefined entity references; null means malformed.
    public function decodeText(string $raw, bool $attribute): ?string
    {
        if ($attribute) {
            $raw = str_replace(["\t", "\n", "\r"], ' ', $raw);
        } elseif (strpos($raw, "\r") !== false) {
            $raw = str_replace("\r\n", "\n", $raw);
            $raw = str_replace("\r", "\n", $raw);
        }
        if (strpos($raw, '&') === false) {
            return $raw;
        }
        $out = '';
        $length = strlen($raw);
        $i = 0;
        while ($i < $length) {
            $amp = strpos($raw, '&', $i);
            if ($amp === false) {
                $out .= substr($raw, $i);
                break;
            }
            $out .= substr($raw, $i, $amp - $i);
            $semi = strpos($raw, ';', $amp);
            if ($semi === false) {
                return null;
            }
            $decoded = __ElephcDom::decodeReference(substr($raw, $amp + 1, $semi - $amp - 1));
            if ($decoded === null) {
                return null;
            }
            $out .= $decoded;
            $i = $semi + 1;
        }
        return $out;
    }
}

final class __ElephcHtmlTables
{
    public static function entityByName(): array
    {
        $map = [];
        foreach (explode(',', 'quot:34,amp:38,lt:60,gt:62,nbsp:160,iexcl:161,cent:162,pound:163,curren:164,yen:165,brvbar:166,sect:167,uml:168,copy:169,ordf:170,laquo:171,not:172,shy:173,reg:174,macr:175,deg:176,plusmn:177,sup2:178,sup3:179,acute:180,micro:181,para:182,middot:183,cedil:184,sup1:185,ordm:186,raquo:187,frac14:188,frac12:189,frac34:190,iquest:191,Agrave:192,Aacute:193,Acirc:194,Atilde:195,Auml:196,Aring:197,AElig:198,Ccedil:199,Egrave:200,Eacute:201,Ecirc:202,Euml:203,Igrave:204,Iacute:205,Icirc:206,Iuml:207,ETH:208,Ntilde:209,Ograve:210,Oacute:211,Ocirc:212,Otilde:213,Ouml:214,times:215,Oslash:216,Ugrave:217,Uacute:218,Ucirc:219,Uuml:220,Yacute:221,THORN:222,szlig:223,agrave:224,aacute:225,acirc:226,atilde:227,auml:228,aring:229,aelig:230,ccedil:231,egrave:232,eacute:233,ecirc:234,euml:235,igrave:236,iacute:237,icirc:238,iuml:239,eth:240,ntilde:241,ograve:242,oacute:243,ocirc:244,otilde:245,ouml:246,divide:247,oslash:248,ugrave:249,uacute:250,ucirc:251,uuml:252,yacute:253,thorn:254,yuml:255,OElig:338,oelig:339,Scaron:352,scaron:353,Yuml:376,fnof:402,circ:710,tilde:732,Alpha:913,Beta:914,Gamma:915,Delta:916,Epsilon:917,Zeta:918,Eta:919,Theta:920,Iota:921,Kappa:922,Lambda:923,Mu:924,Nu:925,Xi:926,Omicron:927,Pi:928,Rho:929,Sigma:931,Tau:932,Upsilon:933,Phi:934,Chi:935,Psi:936,Omega:937,alpha:945,beta:946,gamma:947,delta:948,epsilon:949,zeta:950,eta:951,theta:952,iota:953,kappa:954,lambda:955,mu:956,nu:957,xi:958,omicron:959,pi:960,rho:961,sigmaf:962,sigma:963,tau:964,upsilon:965,phi:966,chi:967,psi:968,omega:969,thetasym:977,upsih:978,piv:982,ensp:8194,emsp:8195,thinsp:8201,zwnj:8204,zwj:8205,lrm:8206,rlm:8207,ndash:8211,mdash:8212,lsquo:8216,rsquo:8217,sbquo:8218,ldquo:8220,rdquo:8221,bdquo:8222,dagger:8224,Dagger:8225,bull:8226,hellip:8230,permil:8240,prime:8242,Prime:8243,lsaquo:8249,rsaquo:8250,oline:8254,frasl:8260,euro:8364,image:8465,weierp:8472,real:8476,trade:8482,alefsym:8501,larr:8592,uarr:8593,rarr:8594,darr:8595,harr:8596,crarr:8629,lArr:8656,uArr:8657,rArr:8658,dArr:8659,hArr:8660,forall:8704,part:8706,exist:8707,empty:8709,nabla:8711,isin:8712,notin:8713,ni:8715,prod:8719,sum:8721,minus:8722,lowast:8727,radic:8730,prop:8733,infin:8734,ang:8736,and:8743,or:8744,cap:8745,cup:8746,int:8747,there4:8756,sim:8764,cong:8773,asymp:8776,ne:8800,equiv:8801,le:8804,ge:8805,sub:8834,sup:8835,nsub:8836,sube:8838,supe:8839,oplus:8853,otimes:8855,perp:8869,sdot:8901,lceil:8968,rceil:8969,lfloor:8970,rfloor:8971,lang:9001,rang:9002,loz:9674,spades:9824,clubs:9827,hearts:9829,diams:9830,apos:39') as $pair) {
            $parts = explode(':', $pair);
            $map[$parts[0]] = (int) $parts[1];
        }
        return $map;
    }

    public static function entityByValue(): array
    {
        $map = [];
        foreach (explode(',', 'quot:34,amp:38,lt:60,gt:62,nbsp:160,iexcl:161,cent:162,pound:163,curren:164,yen:165,brvbar:166,sect:167,uml:168,copy:169,ordf:170,laquo:171,not:172,shy:173,reg:174,macr:175,deg:176,plusmn:177,sup2:178,sup3:179,acute:180,micro:181,para:182,middot:183,cedil:184,sup1:185,ordm:186,raquo:187,frac14:188,frac12:189,frac34:190,iquest:191,Agrave:192,Aacute:193,Acirc:194,Atilde:195,Auml:196,Aring:197,AElig:198,Ccedil:199,Egrave:200,Eacute:201,Ecirc:202,Euml:203,Igrave:204,Iacute:205,Icirc:206,Iuml:207,ETH:208,Ntilde:209,Ograve:210,Oacute:211,Ocirc:212,Otilde:213,Ouml:214,times:215,Oslash:216,Ugrave:217,Uacute:218,Ucirc:219,Uuml:220,Yacute:221,THORN:222,szlig:223,agrave:224,aacute:225,acirc:226,atilde:227,auml:228,aring:229,aelig:230,ccedil:231,egrave:232,eacute:233,ecirc:234,euml:235,igrave:236,iacute:237,icirc:238,iuml:239,eth:240,ntilde:241,ograve:242,oacute:243,ocirc:244,otilde:245,ouml:246,divide:247,oslash:248,ugrave:249,uacute:250,ucirc:251,uuml:252,yacute:253,thorn:254,yuml:255,OElig:338,oelig:339,Scaron:352,scaron:353,Yuml:376,fnof:402,circ:710,tilde:732,Alpha:913,Beta:914,Gamma:915,Delta:916,Epsilon:917,Zeta:918,Eta:919,Theta:920,Iota:921,Kappa:922,Lambda:923,Mu:924,Nu:925,Xi:926,Omicron:927,Pi:928,Rho:929,Sigma:931,Tau:932,Upsilon:933,Phi:934,Chi:935,Psi:936,Omega:937,alpha:945,beta:946,gamma:947,delta:948,epsilon:949,zeta:950,eta:951,theta:952,iota:953,kappa:954,lambda:955,mu:956,nu:957,xi:958,omicron:959,pi:960,rho:961,sigmaf:962,sigma:963,tau:964,upsilon:965,phi:966,chi:967,psi:968,omega:969,thetasym:977,upsih:978,piv:982,ensp:8194,emsp:8195,thinsp:8201,zwnj:8204,zwj:8205,lrm:8206,rlm:8207,ndash:8211,mdash:8212,lsquo:8216,rsquo:8217,sbquo:8218,ldquo:8220,rdquo:8221,bdquo:8222,dagger:8224,Dagger:8225,bull:8226,hellip:8230,permil:8240,prime:8242,Prime:8243,lsaquo:8249,rsaquo:8250,oline:8254,frasl:8260,euro:8364,image:8465,weierp:8472,real:8476,trade:8482,alefsym:8501,larr:8592,uarr:8593,rarr:8594,darr:8595,harr:8596,crarr:8629,lArr:8656,uArr:8657,rArr:8658,dArr:8659,hArr:8660,forall:8704,part:8706,exist:8707,empty:8709,nabla:8711,isin:8712,notin:8713,ni:8715,prod:8719,sum:8721,minus:8722,lowast:8727,radic:8730,prop:8733,infin:8734,ang:8736,and:8743,or:8744,cap:8745,cup:8746,int:8747,there4:8756,sim:8764,cong:8773,asymp:8776,ne:8800,equiv:8801,le:8804,ge:8805,sub:8834,sup:8835,nsub:8836,sube:8838,supe:8839,oplus:8853,otimes:8855,perp:8869,sdot:8901,lceil:8968,rceil:8969,lfloor:8970,rfloor:8971,lang:9001,rang:9002,loz:9674,spades:9824,clubs:9827,hearts:9829,diams:9830,apos:39') as $pair) {
            $parts = explode(':', $pair);
            $map[(int) $parts[1]] = $parts[0];
        }
        return $map;
    }

    public static function closeMap(): array
    {
        $map = [];
        foreach (explode(' ', 'a>a a>fieldset a>table a>td a>th address>dd address>dl address>dt address>form address>li address>ul b>center b>p b>td b>th big>p caption>col caption>colgroup caption>tbody caption>tfoot caption>thead caption>tr colgroup>colgroup colgroup>tbody colgroup>tfoot colgroup>thead colgroup>tr dd>dt dir>dd dir>dl dir>dt dir>form dir>ul dl>form dl>li dt>dd dt>dl font>center font>td font>th form>form h1>fieldset h1>form h1>li h1>p h1>table h2>fieldset h2>form h2>li h2>p h2>table h3>fieldset h3>form h3>li h3>p h3>table h4>fieldset h4>form h4>li h4>p h4>table h5>fieldset h5>form h5>li h5>p h5>table h6>fieldset h6>form h6>li h6>p h6>table head>a head>abbr head>acronym head>address head>b head>bdo head>big head>blockquote head>body head>br head>center head>cite head>code head>dd head>dfn head>dir head>div head>dl head>dt head>em head>fieldset head>font head>form head>frameset head>h1 head>h2 head>h3 head>h4 head>h5 head>h6 head>hr head>i head>iframe head>img head>kbd head>li head>map head>menu head>ol head>p head>pre head>q head>s head>samp head>small head>span head>strike head>strong head>sub head>sup head>table head>tt head>u head>ul head>var i>center i>p i>td i>th legend>fieldset li>li menu>dd menu>dl menu>dt menu>form menu>ul ol>form ol>ul option>optgroup option>option p>address p>blockquote p>body p>caption p>center p>col p>colgroup p>dd p>dir p>div p>dl p>dt p>fieldset p>form p>frameset p>h1 p>h2 p>h3 p>h4 p>h5 p>h6 p>head p>hr p>li p>menu p>ol p>p p>pre p>table p>tbody p>td p>tfoot p>th p>title p>tr p>ul pre>dd pre>dl pre>dt pre>fieldset pre>form pre>li pre>table pre>ul s>p small>p span>td span>th strike>p tbody>tbody tbody>tfoot td>tbody td>td td>tfoot td>th td>tr tfoot>tbody th>tbody th>td th>tfoot th>th th>tr thead>tbody thead>tfoot title>body title>frameset tr>tbody tr>tfoot tr>tr tt>p u>p u>td u>th ul>address ul>form ul>menu ul>ol ul>pre script>noscript style>body style>frameset') as $pair) {
            $map[$pair] = true;
        }
        return $map;
    }

    public static function words(string $list): array
    {
        $map = [];
        foreach (explode(' ', $list) as $word) {
            $map[$word] = true;
        }
        return $map;
    }

    public static function voidTags(): array
    {
        return self::words('area base basefont br col frame hr img input isindex link meta param');
    }

    public static function knownTags(): array
    {
        return self::words('a abbr acronym address applet area b base basefont bdo big blockquote body br button caption center cite code col colgroup dd del dfn dir div dl dt em embed fieldset font form frame frameset h1 h2 h3 h4 h5 h6 head hr html i iframe img input ins isindex kbd label legend li link map menu meta noframes noscript object ol optgroup option p param pre q s samp script select small span strike strong style sub sup table tbody td textarea tfoot th thead title tr tt u ul var');
    }

    public static function blockTags(): array
    {
        return self::words('address area base blockquote body caption center col colgroup dd dir div dl dt fieldset form frame frameset h1 h2 h3 h4 h5 h6 head hr html isindex legend li link menu meta noframes noscript ol optgroup option p param pre style table tbody td tfoot th thead title tr ul');
    }

    public static function booleanAttrs(): array
    {
        return self::words('checked compact declare defer disabled ismap multiple nohref noresize noshade nowrap readonly selected');
    }

    public static function allowPcdata(): array
    {
        return self::words('a abbr acronym address applet b bdo big blockquote body button caption center cite code dd del dfn div dt em font form h1 h2 h3 h4 h5 h6 i iframe ins kbd label legend li noscript object p pre q s samp small span strike strong td th tt u var');
    }

    public static function endPriority(string $name): int
    {
        if ($name === 'div') {
            return 150;
        }
        if ($name === 'td' || $name === 'th') {
            return 160;
        }
        if ($name === 'tr') {
            return 170;
        }
        if ($name === 'thead' || $name === 'tbody' || $name === 'tfoot') {
            return 180;
        }
        if ($name === 'table') {
            return 220;
        }
        if ($name === 'head') {
            return 230;
        }
        if ($name === 'body') {
            return 240;
        }
        if ($name === 'html') {
            return 250;
        }
        return 100;
    }
}

final class __ElephcHtmlParser
{
    public DOMDocument $doc;
    public string $s = '';
    public int $pos = 0;
    public int $len = 0;
    public int $options = 0;
    public array $names = [];
    public array $nodes = [];
    public int $nodeTop = 0;
    public int $depth = 0;
    public int $seen = 0;
    public array $closeMap = [];
    public array $voidTags = [];
    public array $entities = [];
    public array $booleanAttrs = [];
    public array $allowPcdata = [];
    public bool $keepBlanks = true;

    public function __construct(DOMDocument $doc, string $source, int $options)
    {
        $this->doc = $doc;
        $this->options = $options;
        $this->keepBlanks = ($options & 256) === 0;
        $this->s = self::decodeSource($source);
        $this->len = strlen($this->s);
    }

    // Latin-1 unless the document names UTF-8 itself (an xml prolog first, or a meta charset).
    // Plain string scanning: a regex here would drag the pcre2 package into every DOM program.
    public static function decodeSource(string $source): string
    {
        if (!self::hasHighByte($source)) {
            return $source;
        }
        if (str_starts_with($source, '<?xml')) {
            return $source;
        }
        $cut = self::metaUtf8End($source);
        if ($cut < 0) {
            return self::latin1ToUtf8($source);
        }
        return self::latin1ToUtf8(substr($source, 0, $cut)) . substr($source, $cut);
    }

    public static function hasHighByte(string $text): bool
    {
        $length = strlen($text);
        for ($i = 0; $i < $length; $i++) {
            if (ord($text[$i]) >= 128) {
                return true;
            }
        }
        return false;
    }

    // Offset just past the first <meta> start tag that declares a UTF-8 charset, or -1.
    public static function metaUtf8End(string $source): int
    {
        $lower = strtolower($source);
        $from = 0;
        while (true) {
            $at = strpos($lower, '<meta', $from);
            if ($at === false) {
                return -1;
            }
            $end = strpos($lower, '>', $at);
            if ($end === false) {
                $end = strlen($lower);
            }
            $next = substr($lower, $at + 5, 1);
            $tag = substr($lower, $at, $end - $at);
            $charset = strpos($tag, 'charset');
            if ($charset !== false && ($next === '' || !(($next >= '0' && $next <= '9') || ($next >= 'a' && $next <= 'z') || $next === '_'))) {
                $rest = ltrim(substr($tag, $charset + 7), " \t\r\n");
                if ($rest !== '' && $rest[0] === '=') {
                    $rest = ltrim(substr($rest, 1), " \t\r\n\"'");
                    if (str_starts_with($rest, 'utf-8') || str_starts_with($rest, 'utf8')) {
                        return $end + 1;
                    }
                }
            }
            $from = $at + 5;
        }
    }

    public static function latin1ToUtf8(string $text): string
    {
        $out = '';
        $length = strlen($text);
        for ($i = 0; $i < $length; $i++) {
            $code = ord($text[$i]);
            if ($code < 128) {
                $out .= $text[$i];
            } else {
                $out .= chr(0xC0 | ($code >> 6)) . chr(0x80 | ($code & 0x3F));
            }
        }
        return $out;
    }

    public function cur(): string
    {
        return $this->pos < $this->len ? $this->s[$this->pos] : '';
    }

    public function nxt(int $n): string
    {
        return $this->pos + $n < $this->len ? $this->s[$this->pos + $n] : '';
    }

    public function skipBlanks(): void
    {
        while ($this->pos < $this->len) {
            $c = $this->s[$this->pos];
            if ($c === ' ' || $c === "\t" || $c === "\n" || $c === "\r") {
                $this->pos++;
            } else {
                break;
            }
        }
    }

    public function isBlankChar(string $c): bool
    {
        return $c === ' ' || $c === "\t" || $c === "\n" || $c === "\r";
    }

    public function isLetter(string $c): bool
    {
        return ($c >= 'a' && $c <= 'z') || ($c >= 'A' && $c <= 'Z');
    }

    public function startsWithNoCase(string $word): bool
    {
        return strtolower(substr($this->s, $this->pos, strlen($word))) === $word;
    }

    public function topName(): ?string
    {
        $n = count($this->names);
        return $n === 0 ? null : $this->names[$n - 1];
    }

    public function topNode(): ?DOMNode
    {
        // The stack is a slot array plus a depth: array_pop() of an object array leaks the popped object.
        return $this->nodeTop === 0 ? null : $this->nodes[$this->nodeTop - 1];
    }

    public function parse(): bool
    {
        $this->closeMap = __ElephcHtmlTables::closeMap();
        $this->voidTags = __ElephcHtmlTables::voidTags();
        $this->entities = __ElephcHtmlTables::entityByName();
        $this->booleanAttrs = __ElephcHtmlTables::booleanAttrs();
        $this->allowPcdata = __ElephcHtmlTables::allowPcdata();
        $doc = $this->doc;
        $doc->isHtml = true;
        $doc->nodeType = 13;
        $doc->standaloneSet = true;
        $doc->xmlStandalone = true;
        $doc->standalone = true;
        $this->skipBlanks();
        while (($this->cur() === '<' && $this->nxt(1) === '!' && $this->nxt(2) === '-' && $this->nxt(3) === '-') || ($this->cur() === '<' && $this->nxt(1) === '?')) {
            if ($this->cur() === '<' && $this->nxt(1) === '!') {
                $this->parseComment();
            } else {
                $this->parsePI();
            }
            $this->skipBlanks();
        }
        if ($this->cur() === '<' && $this->nxt(1) === '!' && $this->startsWithNoCaseAt(2, 'doctype')) {
            $this->parseDoctype();
        }
        $this->skipBlanks();
        while (($this->cur() === '<' && $this->nxt(1) === '!' && $this->nxt(2) === '-' && $this->nxt(3) === '-') || ($this->cur() === '<' && $this->nxt(1) === '?')) {
            if ($this->cur() === '<' && $this->nxt(1) === '!') {
                $this->parseComment();
            } else {
                $this->parsePI();
            }
            $this->skipBlanks();
        }
        $this->parseContent();
        if ($this->pos >= $this->len) {
            $this->autoCloseOnEnd();
        }
        if (($this->options & 4) === 0 && $doc->doctype === null) {
            $dtd = new DOMDocumentType('html', '-//W3C//DTD HTML 4.0 Transitional//EN', 'http://www.w3.org/TR/REC-html40/loose.dtd');
            $dtd->ownerDocument = $doc;
            $before = $doc->firstChild;
            $doc->linkBefore($dtd, $before);
        }
        return true;
    }

    public function startsWithNoCaseAt(int $offset, string $word): bool
    {
        return strtolower(substr($this->s, $this->pos + $offset, strlen($word))) === $word;
    }

    // ---- SAX layer: the tree the libxml2 default handlers build ----

    public function saxStartElement(string $name, array $attrs, array $valueless): void
    {
        $local = $name;
        $colon = strpos($name, ':');
        if ($colon !== false && $colon > 0) {
            $local = substr($name, $colon + 1);
        }
        $local = $local === '' ? $name : $local;
        $element = new DOMElement('x');
        $element->nodeName = $local;
        $element->tagName = $local;
        $element->localName = $local;
        $element->ownerDocument = $this->doc;
        foreach ($attrs as $attrName => $value) {
            $attr = new DOMAttr((string) $attrName, $value);
            $attr->ownerDocument = $this->doc;
            $attr->ownerElement = $element;
            $attr->parentNode = $element;
            if (isset($valueless[(string) $attrName])) {
                $attr->valueless = true;
            }
            $element->attrNodes[(string) $attrName] = $attr;
        }
        $element->syncAttrCount();
        $parent = $this->topNode();
        if ($this->doc->firstChild === null) {
            $this->doc->linkBefore($element, null);
        } else {
            if ($parent === null) {
                $parent = $this->doc->firstChild;
            }
            if ($parent->nodeType === 1) {
                $parent->linkBefore($element, null);
            } else {
                $this->doc->linkBefore($element, null);
            }
        }
        if (count($this->nodes) > $this->nodeTop) {
            $this->nodes[$this->nodeTop] = $element;
        } else {
            $this->nodes[] = $element;
        }
        $this->nodeTop++;
    }

    public function saxEndElement(): void
    {
        $this->nodeTop--;
        $this->nodes[$this->nodeTop] = null;
    }

    public function saxCharacters(string $text): void
    {
        $node = $this->topNode();
        if ($node === null || $text === '') {
            return;
        }
        $last = $node->lastChild;
        if ($last !== null && $last->nodeType === 3) {
            $last->appendData($text);
            return;
        }
        $created = new DOMText($text);
        $created->ownerDocument = $this->doc;
        $node->linkBefore($created, null);
    }

    public function saxCdata(string $text): void
    {
        $node = $this->topNode();
        if ($node === null || $text === '') {
            return;
        }
        $last = $node->lastChild;
        if ($last !== null && $last->nodeType === 4) {
            $last->appendData($text);
            return;
        }
        $created = new DOMCdataSection($text);
        $created->ownerDocument = $this->doc;
        $node->linkBefore($created, null);
    }

    public function saxAddMisc(DOMNode $node): void
    {
        $parent = $this->topNode();
        $node->ownerDocument = $this->doc;
        if ($parent === null || $parent->nodeType !== 1) {
            $this->doc->linkBefore($node, null);
        } else {
            $parent->linkBefore($node, null);
        }
    }

    public function namePush(string $name): void
    {
        if ($this->seen < 3 && $name === 'head') {
            $this->seen = 3;
        }
        if ($this->seen < 10 && $name === 'body') {
            $this->seen = 10;
        }
        $this->names[] = $name;
    }

    public function namePop(): void
    {
        array_pop($this->names);
    }

    // ---- implied structure ----

    public function checkAutoClose(string $newTag, string $oldTag): bool
    {
        return isset($this->closeMap[$oldTag . '>' . $newTag]);
    }

    public function autoClose(string $newTag): void
    {
        while (count($this->names) > 0 && $this->checkAutoClose($newTag, $this->names[count($this->names) - 1])) {
            $this->saxEndElement();
            $this->namePop();
        }
    }

    public function autoCloseOnEnd(): void
    {
        while (count($this->names) > 0) {
            $this->saxEndElement();
            $this->namePop();
        }
    }

    public function autoCloseOnClose(string $newTag): void
    {
        $priority = __ElephcHtmlTables::endPriority($newTag);
        $i = count($this->names) - 1;
        while ($i >= 0) {
            if ($this->names[$i] === $newTag) {
                break;
            }
            if (__ElephcHtmlTables::endPriority($this->names[$i]) > $priority) {
                return;
            }
            $i--;
        }
        if ($i < 0) {
            return;
        }
        while ($this->topName() !== $newTag) {
            $this->saxEndElement();
            $this->namePop();
        }
    }

    public function checkImplied(string $newTag): void
    {
        if (($this->options & 8192) !== 0) {
            return;
        }
        if ($newTag === 'html') {
            return;
        }
        if (count($this->names) <= 0) {
            $this->namePush('html');
            $this->saxStartElement('html', [], []);
        }
        if ($newTag === 'body' || $newTag === 'head') {
            return;
        }
        if (count($this->names) <= 1 && ($newTag === 'script' || $newTag === 'style' || $newTag === 'meta' || $newTag === 'link' || $newTag === 'title' || $newTag === 'base')) {
            if ($this->seen >= 3) {
                return;
            }
            $this->namePush('head');
            $this->saxStartElement('head', [], []);
        } elseif ($newTag !== 'noframes' && $newTag !== 'frame' && $newTag !== 'frameset') {
            if ($this->seen >= 10) {
                return;
            }
            foreach ($this->names as $open) {
                if ($open === 'body' || $open === 'head') {
                    return;
                }
            }
            $this->namePush('body');
            $this->saxStartElement('body', [], []);
        }
    }

    public function checkParagraph(): void
    {
        $tag = $this->topName();
        if ($tag === null) {
            $this->autoClose('p');
            $this->checkImplied('p');
            $this->namePush('p');
            $this->saxStartElement('p', [], []);
            return;
        }
        if ($tag === 'html' || $tag === 'head') {
            $this->autoClose('p');
            $this->checkImplied('p');
            $this->namePush('p');
            $this->saxStartElement('p', [], []);
        }
    }

    // ---- the content loop ----

    public function parseContent(): void
    {
        $currentNode = $this->topName();
        $depth = count($this->names);
        while (true) {
            $c = $this->pos < $this->len ? $this->s[$this->pos] : '';
            if ($c === '<' && $this->nxt(1) === '/') {
                $ended = $this->parseEndTag();
                if ($ended && ($currentNode !== null || count($this->names) === 0)) {
                    $currentNode = $this->topName();
                    $depth = count($this->names);
                }
                continue;
            }
            $top = $this->topName();
            if ($c === '<' && $top !== null && ($this->isLetter($this->nxt(1)) || $this->nxt(1) === '_' || $this->nxt(1) === ':')) {
                $save = $this->pos;
                $this->pos++;
                $peek = $this->parseHtmlName();
                $this->pos = $save;
                if ($peek !== null && $this->checkAutoClose($peek, $top)) {
                    $this->autoClose($peek);
                    continue;
                }
            }
            if (count($this->names) > 0 && $depth >= count($this->names) && $currentNode !== $top) {
                $currentNode = $top;
                $depth = count($this->names);
                continue;
            }
            if ($c !== '' && ($currentNode === 'script' || $currentNode === 'style')) {
                $this->parseScript();
                continue;
            }
            if ($c === '') {
                $this->autoCloseOnEnd();
                break;
            }
            if ($c === '<' && $this->nxt(1) === '!') {
                if ($this->startsWithNoCaseAt(2, 'doctype')) {
                    $this->parseDoctype();
                } elseif ($this->nxt(2) === '-' && $this->nxt(3) === '-') {
                    $this->parseComment();
                } else {
                    $this->skipBogusComment();
                }
            } elseif ($c === '<' && $this->nxt(1) === '?') {
                $this->parsePI();
            } elseif ($c === '<') {
                $this->parseElement();
                $currentNode = $this->topName();
                $depth = count($this->names);
            } elseif ($c === '&') {
                $this->parseReference();
            } else {
                $this->parseCharData();
            }
        }
    }

    public function skipBogusComment(): void
    {
        while ($this->pos < $this->len && $this->s[$this->pos] !== '>') {
            $this->pos++;
        }
        if ($this->pos < $this->len) {
            $this->pos++;
        }
    }

    public function parseScript(): void
    {
        $start = $this->pos;
        while ($this->pos < $this->len) {
            if ($this->s[$this->pos] === '<' && $this->nxt(1) === '/' && $this->isLetter($this->nxt(2))) {
                break;
            }
            $this->pos++;
        }
        $this->saxCdata(substr($this->s, $start, $this->pos - $start));
    }

    public function parseComment(): void
    {
        $end = strpos($this->s, '-->', $this->pos + 4);
        if ($end === false) {
            $this->pos = $this->len;
            return;
        }
        $comment = new DOMComment(substr($this->s, $this->pos + 4, $end - $this->pos - 4));
        $this->pos = $end + 3;
        $this->saxAddMisc($comment);
    }

    public function parseName(): ?string
    {
        $c = $this->cur();
        if ($c === '' || !($this->isLetter($c) || $c === '_' || $c === ':' || ord($c) >= 128)) {
            return null;
        }
        $start = $this->pos;
        while ($this->pos < $this->len) {
            $c = $this->s[$this->pos];
            if ($this->isLetter($c) || ($c >= '0' && $c <= '9') || $c === '.' || $c === '-' || $c === '_' || $c === ':' || ord($c) >= 128) {
                $this->pos++;
            } else {
                break;
            }
        }
        return substr($this->s, $start, $this->pos - $start);
    }

    public function parsePI(): void
    {
        $this->pos += 2;
        $target = $this->parseName();
        if ($target === null) {
            return;
        }
        if ($this->pos >= $this->len) {
            $this->checkParagraph();
            $this->saxCharacters($target);
            return;
        }
        if ($this->cur() === '>') {
            $this->pos++;
            $this->saxAddMisc(new DOMProcessingInstruction($target, ''));
            return;
        }
        $this->skipBlanks();
        $start = $this->pos;
        while ($this->pos < $this->len && $this->s[$this->pos] !== '>') {
            $this->pos++;
        }
        if ($this->pos >= $this->len) {
            return;
        }
        $data = substr($this->s, $start, $this->pos - $start);
        $this->pos++;
        $this->saxAddMisc(new DOMProcessingInstruction($target, $data));
    }

    public function parseLiteral(): ?string
    {
        $quote = $this->cur();
        if ($quote !== '"' && $quote !== "'") {
            return null;
        }
        $end = strpos($this->s, $quote, $this->pos + 1);
        if ($end === false) {
            $this->pos = $this->len;
            return null;
        }
        $value = substr($this->s, $this->pos + 1, $end - $this->pos - 1);
        $this->pos = $end + 1;
        return $value;
    }

    public function parseDoctype(): void
    {
        $this->pos += 9;
        $this->skipBlanks();
        $name = $this->parseName();
        $this->skipBlanks();
        $public = null;
        $system = null;
        if ($this->startsWithNoCase('system')) {
            $this->pos += 6;
            $this->skipBlanks();
            $system = $this->parseLiteral();
        } elseif ($this->startsWithNoCase('public')) {
            $this->pos += 6;
            $this->skipBlanks();
            $public = $this->parseLiteral();
            $this->skipBlanks();
            if ($this->cur() === '"' || $this->cur() === "'") {
                $system = $this->parseLiteral();
            }
        }
        $this->skipBlanks();
        while ($this->pos < $this->len && $this->s[$this->pos] !== '>') {
            $this->pos++;
        }
        if ($this->cur() === '>') {
            $this->pos++;
        }
        if ($name === null || $this->doc->doctype !== null) {
            return;
        }
        $dtd = new DOMDocumentType($name, $public === null ? '' : $public, $system === null ? '' : $system);
        $dtd->ownerDocument = $this->doc;
        $before = $this->doc->firstChild;
        $this->doc->linkBefore($dtd, $before);
    }

    // ---- character data and references ----

    public function areBlanks(string $text): bool
    {
        $length = strlen($text);
        for ($i = 0; $i < $length; $i++) {
            if (!$this->isBlankChar($text[$i])) {
                return false;
            }
        }
        if ($this->pos >= $this->len) {
            return true;
        }
        if ($this->s[$this->pos] !== '<') {
            return false;
        }
        $name = $this->topName();
        if ($name === null || $name === 'html' || $name === 'head') {
            return true;
        }
        if ($name === 'body') {
            $dtd = $this->doc->doctype;
            if ($dtd !== null && $dtd->publicId !== '') {
                $id = strtolower($dtd->publicId);
                if ($id === '-//w3c//dtd html 4.01//en' || $id === '-//w3c//dtd html 4//en') {
                    return true;
                }
            }
        }
        $node = $this->topNode();
        if ($node === null) {
            return false;
        }
        $last = $node->lastChild;
        while ($last !== null && $last->nodeType === 8) {
            $last = $last->previousSibling;
        }
        if ($last === null) {
            if (isset($this->allowPcdata[$name])) {
                return false;
            }
        } elseif ($last->nodeType === 3 || $last->nodeType === 4) {
            return false;
        } elseif (isset($this->allowPcdata[$last->nodeName])) {
            return false;
        }
        return true;
    }

    public function parseCharData(): void
    {
        $buf = '';
        while ($this->pos < $this->len) {
            $c = $this->s[$this->pos];
            if ($c === '<' || $c === '&') {
                break;
            }
            $code = ord($c);
            if ($code >= 32 || $code === 9 || $code === 10 || $code === 13) {
                $buf .= $c;
            }
            $this->pos++;
        }
        if ($buf === '') {
            return;
        }
        if ($this->areBlanks($buf)) {
            if ($this->keepBlanks) {
                $this->saxCharacters($buf);
            }
        } else {
            $this->checkParagraph();
            $this->saxCharacters($buf);
        }
    }

    public function parseCharRef(): int
    {
        $value = 0;
        if (($this->nxt(2) === 'x' || $this->nxt(2) === 'X')) {
            $this->pos += 3;
            while ($this->pos < $this->len && $this->s[$this->pos] !== ';') {
                $c = $this->s[$this->pos];
                if ($c >= '0' && $c <= '9') {
                    $value = $value * 16 + (ord($c) - 48);
                } elseif ($c >= 'a' && $c <= 'f') {
                    $value = $value * 16 + (ord($c) - 87);
                } elseif ($c >= 'A' && $c <= 'F') {
                    $value = $value * 16 + (ord($c) - 55);
                } else {
                    break;
                }
                if ($value > 0x110000) {
                    $value = 0x110000;
                }
                $this->pos++;
            }
            if ($this->cur() === ';') {
                $this->pos++;
            }
        } else {
            $this->pos += 2;
            while ($this->pos < $this->len && $this->s[$this->pos] !== ';') {
                $c = $this->s[$this->pos];
                if ($c >= '0' && $c <= '9') {
                    $value = $value * 10 + (ord($c) - 48);
                } else {
                    break;
                }
                if ($value > 0x110000) {
                    $value = 0x110000;
                }
                $this->pos++;
            }
            if ($this->cur() === ';') {
                $this->pos++;
            }
        }
        if ($value === 0 || $value >= 0x110000 || ($value < 32 && $value !== 9 && $value !== 10 && $value !== 13) || ($value >= 0xD800 && $value <= 0xDFFF) || $value === 0xFFFE || $value === 0xFFFF) {
            return 0;
        }
        return $value;
    }

    // Returns [name|null, value|null]; consumes the semicolon only for a known entity.
    public function parseEntityRef(): array
    {
        $this->pos++;
        $name = $this->parseName();
        if ($name === null) {
            return [null, null];
        }
        if ($this->cur() === ';') {
            if (isset($this->entities[$name])) {
                $this->pos++;
                return [$name, $this->entities[$name]];
            }
            return [$name, null];
        }
        return [$name, null];
    }

    public function parseReference(): void
    {
        if ($this->nxt(1) === '#') {
            $value = $this->parseCharRef();
            if ($value === 0) {
                return;
            }
            $this->checkParagraph();
            $this->saxCharacters(__ElephcDom::utf8($value));
            return;
        }
        $ref = $this->parseEntityRef();
        if ($ref[0] === null) {
            $this->checkParagraph();
            $this->saxCharacters('&');
            return;
        }
        if ($ref[1] === null) {
            $this->checkParagraph();
            $this->saxCharacters('&' . $ref[0]);
            return;
        }
        $this->checkParagraph();
        $this->saxCharacters(__ElephcDom::utf8($ref[1]));
    }

    // ---- tags ----

    public function parseHtmlName(): ?string
    {
        $c = $this->cur();
        if ($c === '' || !($this->isLetter($c) || $c === '_' || $c === ':' || $c === '.')) {
            return null;
        }
        $start = $this->pos;
        while ($this->pos < $this->len) {
            $c = $this->s[$this->pos];
            if ($this->isLetter($c) || ($c >= '0' && $c <= '9') || $c === ':' || $c === '-' || $c === '_' || $c === '.') {
                $this->pos++;
            } else {
                break;
            }
        }
        return strtolower(substr($this->s, $start, $this->pos - $start));
    }

    public function parseEndTag(): bool
    {
        $this->pos += 2;
        $name = $this->parseHtmlName();
        if ($name === null) {
            return false;
        }
        $this->skipBlanks();
        if ($this->cur() !== '>') {
            while ($this->pos < $this->len && $this->s[$this->pos] !== '>') {
                $this->pos++;
            }
        }
        if ($this->cur() === '>') {
            $this->pos++;
        }
        if ($this->depth > 0 && ($name === 'html' || $name === 'body' || $name === 'head')) {
            $this->depth--;
            return false;
        }
        $found = false;
        for ($i = count($this->names) - 1; $i >= 0; $i--) {
            if ($this->names[$i] === $name) {
                $found = true;
                break;
            }
        }
        if (!$found) {
            return false;
        }
        $this->autoCloseOnClose($name);
        if ($this->topName() === $name) {
            $this->saxEndElement();
            $this->namePop();
            return true;
        }
        return false;
    }

    public function parseElement(): void
    {
        $failed = $this->parseStartTag();
        $name = $this->topName();
        if ($failed === -1 || $name === null) {
            if ($this->cur() === '>') {
                $this->pos++;
            }
            return;
        }
        if ($this->cur() === '/' && $this->nxt(1) === '>') {
            $this->pos += 2;
            $this->saxEndElement();
            $this->namePop();
            return;
        }
        if ($this->cur() === '>') {
            $this->pos++;
        } else {
            $this->saxEndElement();
            $this->namePop();
            return;
        }
        if (isset($this->voidTags[$name])) {
            $this->saxEndElement();
            $this->namePop();
        }
    }

    // Returns -1 for an invalid tag, 1 for a discarded one, 0 once pushed.
    public function parseStartTag(): int
    {
        $this->pos++;
        $name = $this->parseHtmlName();
        if ($name === null) {
            while ($this->pos < $this->len && $this->s[$this->pos] !== '>') {
                $this->pos++;
            }
            return -1;
        }
        $this->autoClose($name);
        $this->checkImplied($name);
        $discard = false;
        if (count($this->names) > 0 && $name === 'html') {
            $discard = true;
            $this->depth++;
        }
        if (count($this->names) !== 1 && $name === 'head') {
            $discard = true;
            $this->depth++;
        }
        if ($name === 'body') {
            foreach ($this->names as $open) {
                if ($open === 'body') {
                    $discard = true;
                    $this->depth++;
                }
            }
        }
        $this->skipBlanks();
        $attrs = [];
        $valueless = [];
        while ($this->pos < $this->len && $this->s[$this->pos] !== '>' && !($this->s[$this->pos] === '/' && $this->nxt(1) === '>')) {
            $attrName = $this->parseHtmlName();
            if ($attrName !== null) {
                $this->skipBlanks();
                $value = null;
                if ($this->cur() === '=') {
                    $this->pos++;
                    $this->skipBlanks();
                    $value = $this->parseAttValue();
                }
                if (!isset($attrs[$attrName])) {
                    if ($value === null) {
                        if (isset($this->booleanAttrs[$attrName])) {
                            $attrs[$attrName] = $attrName;
                        } else {
                            $attrs[$attrName] = '';
                            $valueless[$attrName] = true;
                        }
                    } else {
                        $attrs[$attrName] = $value;
                    }
                }
            } else {
                while ($this->pos < $this->len && !$this->isBlankChar($this->s[$this->pos]) && $this->s[$this->pos] !== '>' && !($this->s[$this->pos] === '/' && $this->nxt(1) === '>')) {
                    $this->pos++;
                }
            }
            $this->skipBlanks();
        }
        if (!$discard) {
            $this->namePush($name);
            $this->saxStartElement($name, $attrs, $valueless);
        }
        return $discard ? 1 : 0;
    }

    public function parseAttValue(): ?string
    {
        $c = $this->cur();
        if ($c === '"' || $c === "'") {
            $this->pos++;
            $value = $this->parseAttribute($c);
            if ($this->cur() === $c) {
                $this->pos++;
            }
            return $value;
        }
        return $this->parseAttribute('');
    }

    public function parseAttribute(string $stop): string
    {
        $out = '';
        while ($this->pos < $this->len) {
            $c = $this->s[$this->pos];
            if ($stop !== '' && $c === $stop) {
                break;
            }
            if ($stop === '' && ($c === '>' || $this->isBlankChar($c))) {
                break;
            }
            if ($c === '&') {
                if ($this->nxt(1) === '#') {
                    $value = $this->parseCharRef();
                    if ($value !== 0) {
                        $out .= __ElephcDom::utf8($value);
                    }
                } else {
                    $ref = $this->parseEntityRef();
                    if ($ref[0] === null) {
                        $out .= '&';
                    } elseif ($ref[1] === null) {
                        $out .= '&' . $ref[0];
                    } else {
                        $out .= __ElephcDom::utf8($ref[1]);
                    }
                }
                continue;
            }
            $code = ord($c);
            if ($code >= 32 || $code === 9 || $code === 10 || $code === 13) {
                $out .= $c;
            }
            $this->pos++;
        }
        return $out;
    }
}

final class __ElephcDomHtmlWriter
{
    public string $out = '';
    public bool $format = false;
    public int $mode = 0;
    public array $voidTags = [];
    public array $known = [];
    public array $block = [];
    public array $booleans = [];
    public array $byValue = [];

    public static function make(bool $format, int $mode): __ElephcDomHtmlWriter
    {
        $writer = new __ElephcDomHtmlWriter();
        $writer->format = $format;
        $writer->mode = $mode;
        $writer->voidTags = __ElephcHtmlTables::voidTags();
        $writer->known = __ElephcHtmlTables::knownTags();
        $writer->block = __ElephcHtmlTables::blockTags();
        $writer->booleans = __ElephcHtmlTables::booleanAttrs();
        if ($mode === 0) {
            $writer->byValue = __ElephcHtmlTables::entityByValue();
        }
        return $writer;
    }

    public static function document(DOMDocument $doc, bool $format): string
    {
        $writer = self::make($format, 0);
        $child = $doc->firstChild;
        if ($doc->doctype !== null) {
            $dtd = $doc->doctype;
            $writer->out .= self::doctype($dtd);
        }
        while ($child !== null) {
            $writer->node($child);
            $child = $child->nextSibling;
        }
        return $writer->finish($writer->out . "\n");
    }

    public static function dumpNode(DOMDocument $doc, DOMNode $node, bool $format): string
    {
        $mode = 1;
        if ($doc->encoding === null && $doc->nodeType !== 13) {
            $mode = 2;
        }
        $writer = self::make($format, $mode);
        $writer->node($node);
        return $writer->finish($writer->out);
    }

    // The output buffer rewrites what the output encoding cannot carry.
    public function finish(string $text): string
    {
        if ($this->mode === 1) {
            return $text;
        }
        if ($this->mode === 2) {
            return __ElephcDom::asciiRefs($text);
        }
        return $this->entityRefs($text);
    }

    public static function doctype(DOMDocumentType $dtd): string
    {
        $out = '<!DOCTYPE ' . $dtd->name;
        if ($dtd->publicId !== '') {
            $out .= ' PUBLIC ' . self::quoted($dtd->publicId);
            if ($dtd->systemId !== '') {
                $out .= ' ' . self::quoted($dtd->systemId);
            }
        } elseif ($dtd->systemId !== '') {
            $out .= ' SYSTEM ' . self::quoted($dtd->systemId);
        }
        return $out . ">\n";
    }

    public static function quoted(string $value): string
    {
        if (strpos($value, '"') === false) {
            return '"' . $value . '"';
        }
        if (strpos($value, "'") === false) {
            return "'" . $value . "'";
        }
        return '"' . str_replace('"', '&quot;', $value) . '"';
    }

    // Escapes text the way the document's HTML serializer does for its output encoding.
    public function escape(string $text): string
    {
        return str_replace(['&', '<', '>', "\r"], ['&amp;', '&lt;', '&gt;', '&#13;'], $text);
    }

    public function entityRefs(string $text): string
    {
        $length = strlen($text);
        $clean = true;
        for ($i = 0; $i < $length; $i++) {
            if (ord($text[$i]) >= 128) {
                $clean = false;
                break;
            }
        }
        if ($clean) {
            return $text;
        }
        $out = '';
        $i = 0;
        while ($i < $length) {
            $code = ord($text[$i]);
            $size = 1;
            $point = $code;
            if ($code >= 0xF0 && $i + 3 < $length) {
                $point = (($code & 0x07) << 18) | ((ord($text[$i + 1]) & 0x3F) << 12) | ((ord($text[$i + 2]) & 0x3F) << 6) | (ord($text[$i + 3]) & 0x3F);
                $size = 4;
            } elseif ($code >= 0xE0 && $i + 2 < $length) {
                $point = (($code & 0x0F) << 12) | ((ord($text[$i + 1]) & 0x3F) << 6) | (ord($text[$i + 2]) & 0x3F);
                $size = 3;
            } elseif ($code >= 0xC0 && $i + 1 < $length) {
                $point = (($code & 0x1F) << 6) | (ord($text[$i + 1]) & 0x3F);
                $size = 2;
            }
            if ($point < 128) {
                $out .= $text[$i];
            } elseif (isset($this->byValue[$point])) {
                $out .= '&' . $this->byValue[$point] . ';';
            } else {
                $out .= '&#' . $point . ';';
            }
            $i += $size;
        }
        return $out;
    }

    public static function uriEscape(string $value): string
    {
        $out = '';
        $length = strlen($value);
        for ($i = 0; $i < $length; $i++) {
            $c = $value[$i];
            $code = ord($c);
            if (($code >= 48 && $code <= 57) || ($code >= 65 && $code <= 90) || ($code >= 97 && $code <= 122) || strpos("-_.!~*'()@/:=?;#%&,+", $c) !== false) {
                $out .= $c;
            } else {
                $out .= '%' . strtoupper(substr('0' . dechex($code), -2));
            }
        }
        return $out;
    }

    public function attribute(DOMAttr $attr, DOMElement $owner): void
    {
        $this->out .= ' ' . $attr->nodeName;
        if (isset($this->booleans[strtolower($attr->name)]) || $attr->valueless) {
            return;
        }
        $value = $this->escape($attr->value);
        $lower = strtolower($attr->name);
        if ($owner->namespaceURI === null && $attr->namespaceURI === null && ($lower === 'href' || $lower === 'action' || $lower === 'src' || ($lower === 'name' && strtolower($owner->nodeName) === 'a'))) {
            $value = self::uriEscape(ltrim($value, " \t\r\n"));
        }
        $this->out .= '=' . self::quoted($value);
    }

    public function isText(?DOMNode $node): bool
    {
        return $node !== null && ($node->nodeType === 3 || $node->nodeType === 5);
    }

    public function node(DOMNode $cur): void
    {
        if ($cur instanceof DOMDocumentType) {
            return;
        }
        if ($cur instanceof DOMDocument || $cur instanceof DOMDocumentFragment) {
            $child = $cur->firstChild;
            while ($child !== null) {
                $this->node($child);
                $child = $child->nextSibling;
            }
            return;
        }
        if ($cur instanceof DOMCdataSection) {
            $this->out .= $cur->data;
            return;
        }
        if ($cur instanceof DOMText) {
            $parent = $cur->parentNode;
            if ($parent !== null && ($parent->nodeName === 'script' || $parent->nodeName === 'style')) {
                $this->out .= $cur->data;
            } else {
                $this->out .= $this->escape($cur->data);
            }
            return;
        }
        if ($cur instanceof DOMComment) {
            $this->out .= '<!--' . $cur->data . '-->';
            return;
        }
        if ($cur instanceof DOMProcessingInstruction) {
            $this->out .= $cur->data === '' ? '<?' . $cur->target . '>' : '<?' . $cur->target . ' ' . $cur->data . '>';
            return;
        }
        if (!($cur instanceof DOMElement)) {
            return;
        }
        $lower = strtolower($cur->nodeName);
        $known = $cur->namespaceURI === null && isset($this->known[$lower]);
        $this->out .= '<' . $cur->nodeName;
        foreach ($cur->attrNodes as $attr) {
            $this->attribute($attr, $cur);
        }
        $next = $cur->nextSibling;
        $parent = $cur->parentNode;
        $afterNewline = $this->format && $known && isset($this->block[$lower]) && $next !== null && !$this->isText($next) && $parent !== null && $parent->nodeType === 1 && $parent->nodeName !== '' && $parent->nodeName[0] !== 'p';
        if ($known && isset($this->voidTags[$lower])) {
            $this->out .= '>';
            if ($afterNewline) {
                $this->out .= "\n";
            }
            return;
        }
        if ($cur->firstChild === null) {
            if ($known && $lower === 'li') {
                $this->out .= '>';
            } else {
                $this->out .= '></' . $cur->nodeName . '>';
            }
            if ($afterNewline) {
                $this->out .= "\n";
            }
            return;
        }
        $this->out .= '>';
        $first = $cur->firstChild;
        $last = $cur->lastChild;
        $nameOk = $cur->nodeName !== '' && $cur->nodeName[0] !== 'p';
        $blockish = $this->format && $known && isset($this->block[$lower]);
        if ($blockish && !$this->isText($first) && $first !== $last && $nameOk) {
            $this->out .= "\n";
        }
        $child = $first;
        while ($child !== null) {
            $this->node($child);
            $child = $child->nextSibling;
        }
        if ($blockish && !$this->isText($last) && $first !== $last && $nameOk) {
            $this->out .= "\n";
        }
        $this->out .= '</' . $cur->nodeName . '>';
        if ($afterNewline) {
            $this->out .= "\n";
        }
    }
}

final class __ElephcDomState
{
    public static bool $internalErrors = false;
    public static int $documents = 0;
}

function libxml_use_internal_errors(?bool $use_errors = null): bool
{
    $previous = __ElephcDomState::$internalErrors;
    if ($use_errors !== null) {
        __ElephcDomState::$internalErrors = $use_errors;
    }
    return $previous;
}

function libxml_clear_errors(): void
{
}

// php's libxml error record: typed, uninitialized properties, as php declares them.
class LibXMLError
{
    public int $level;
    public int $code;
    public int $column;
    public string $message;
    public string $file;
    public int $line;
}

function libxml_get_errors(): array
{
    // The prelude keeps no libxml error list, so the answer is always empty. The dead append only
    // gives the result its element type (LibXMLError) for callers that read $error->message.
    $errors = [];
    if (count($errors) > 0) {
        $errors[] = new LibXMLError();
    }
    return $errors;
}

function libxml_disable_entity_loader(bool $disable = true): bool
{
    $_unused = $disable;
    return true;
}

"####;
