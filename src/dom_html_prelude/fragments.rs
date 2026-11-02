//! Purpose:
//! The PHP form of the `Dom\\` prelude: the model the declarations in `build` were transcribed
//! from, kept under `cfg(test)` as the node-by-node parse-parity oracle.
//!
//! Called from:
//! - `crate::dom_html_prelude::oracle`.
//!
//! Key details:
//! - Class names are MANGLED (`Dom__Node` for `Dom\\Node`) because the transcriber does not model
//!   namespace statements; the oracle undoes that on the parsed side.
//! - The source was verified against php's own `Dom\HTMLDocument` before it was transcribed.

pub(super) const SRC: &str = r####"<?php

class Dom__NodeList implements \IteratorAggregate, \Countable
{
    public int $length = 0;
    public array $items = [];

    public function __construct(array $items = [])
    {
        $this->items = $items;
        $this->length = count($items);
    }

    public function item(int $index): ?\Dom__Node
    {
        if ($index < 0 || $index >= $this->length) {
            return null;
        }
        return $this->items[$index];
    }

    public function count(): int
    {
        return $this->length;
    }

    public function getIterator(): \Iterator
    {
        return new \ArrayIterator($this->items);
    }
}

class Dom__HTMLCollection implements \IteratorAggregate, \Countable
{
    public int $length = 0;
    public array $items = [];

    public function __construct(array $items = [])
    {
        $this->items = $items;
        $this->length = count($items);
    }

    public function item(int $index): ?\Dom__Element
    {
        if ($index < 0 || $index >= $this->length) {
            return null;
        }
        return $this->items[$index];
    }

    public function namedItem(string $key): ?\Dom__Element
    {
        if ($key === '') {
            return null;
        }
        foreach ($this->items as $element) {
            if ($element->getAttribute('id') === $key) {
                return $element;
            }
        }
        foreach ($this->items as $element) {
            if ($element->getAttribute('name') === $key) {
                return $element;
            }
        }
        return null;
    }

    public function count(): int
    {
        return $this->length;
    }

    public function getIterator(): \Iterator
    {
        return new \ArrayIterator($this->items);
    }
}

class Dom__NamedNodeMap implements \IteratorAggregate, \Countable
{
    public int $length = 0;
    public array $items = [];

    public function __construct(array $items = [])
    {
        $this->items = $items;
        $this->length = count($items);
    }

    public function item(int $index): ?\Dom__Attr
    {
        if ($index < 0 || $index >= $this->length) {
            return null;
        }
        return $this->items[$index];
    }

    public function getNamedItem(string $qualifiedName): ?\Dom__Attr
    {
        foreach ($this->items as $attr) {
            if ($attr->name === $qualifiedName) {
                return $attr;
            }
        }
        $wanted = strtolower($qualifiedName);
        foreach ($this->items as $attr) {
            if ($attr->name === $wanted) {
                return $attr;
            }
        }
        return null;
    }

    public function count(): int
    {
        return $this->length;
    }

    public function getIterator(): \Iterator
    {
        return new \ArrayIterator($this->items);
    }
}

class Dom__Node
{
    public const ELEMENT_NODE = 1;
    public const ATTRIBUTE_NODE = 2;
    public const TEXT_NODE = 3;
    public const CDATA_SECTION_NODE = 4;
    public const ENTITY_REFERENCE_NODE = 5;
    public const ENTITY_NODE = 6;
    public const PROCESSING_INSTRUCTION_NODE = 7;
    public const COMMENT_NODE = 8;
    public const DOCUMENT_NODE = 9;
    public const DOCUMENT_TYPE_NODE = 10;
    public const DOCUMENT_FRAGMENT_NODE = 11;
    public const NOTATION_NODE = 12;
    public const HTML_DOCUMENT_NODE = 13;

    public int $nodeType = 0;
    public string $nodeName = '';
    public string $baseURI = 'about:blank';
    public bool $isConnected = false;
    public ?\Dom__Document $ownerDocument = null;
    public ?\Dom__Node $parentNode = null;
    public ?\Dom__Element $parentElement = null;
    public \Dom__NodeList $childNodes;
    public ?\Dom__Node $firstChild = null;
    public ?\Dom__Node $lastChild = null;
    public ?\Dom__Node $previousSibling = null;
    public ?\Dom__Node $nextSibling = null;
    public ?string $nodeValue = null;
    public ?string $textContent = null;
    public array $kids = [];

    public function __construct()
    {
        $this->childNodes = new \Dom__NodeList([]);
    }

    public function hasChildNodes(): bool
    {
        return count($this->kids) > 0;
    }

    public function isSameNode(\Dom__Node $otherNode): bool
    {
        return $this === $otherNode;
    }

    public function contains(?\Dom__Node $other): bool
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

    public function getRootNode(array $options = []): \Dom__Node
    {
        $root = $this;
        while ($root->parentNode !== null) {
            $root = $root->parentNode;
        }
        return $root;
    }

    public function appendChild(\Dom__Node $node): \Dom__Node
    {
        $this->insertBefore($node, null);
        return $node;
    }

    
    public function isBuilding(): bool
    {
        $doc = $this instanceof \Dom__Document ? $this : $this->ownerDocument;
        return $doc !== null && $doc->building;
    }

    public function insertBefore(\Dom__Node $node, ?\Dom__Node $child = null): \Dom__Node
    {
        if ($node->parentNode !== null) {
            $node->parentNode->removeChild($node);
        }
        $index = count($this->kids);
        if ($child !== null) {
            $found = array_search($child, $this->kids, true);
            if ($found === false) {
                throw new \Exception('Not Found Error', 8);
            }
            $index = $found;
        }
        $node->parentNode = $this;
        $node->parentElement = $this instanceof \Dom__Element ? $this : null;
        if ($child === null && $this->isBuilding()) {
            // appending while the parser builds the tree: constant-time link maintenance
            $last = $this->lastChild;
            $this->kids[] = $node;
            $node->previousSibling = $last;
            $node->nextSibling = null;
            if ($last !== null) {
                $last->nextSibling = $node;
            } else {
                $this->firstChild = $node;
            }
            $this->lastChild = $node;
            return $node;
        }
        array_splice($this->kids, $index, 0, [$node]);
        $this->relink();
        return $node;
    }

    public function removeChild(\Dom__Node $child): \Dom__Node
    {
        $found = array_search($child, $this->kids, true);
        if ($found === false) {
            throw new \Exception('Not Found Error', 8);
        }
        array_splice($this->kids, $found, 1);
        $child->parentNode = null;
        $child->parentElement = null;
        $child->previousSibling = null;
        $child->nextSibling = null;
        $this->relink();
        return $child;
    }

    
    public function relink(): void
    {
        $count = count($this->kids);
        for ($i = 0; $i < $count; $i++) {
            $kid = $this->kids[$i];
            $kid->previousSibling = $i > 0 ? $this->kids[$i - 1] : null;
            $kid->nextSibling = $i < $count - 1 ? $this->kids[$i + 1] : null;
        }
        $this->firstChild = $count > 0 ? $this->kids[0] : null;
        $this->lastChild = $count > 0 ? $this->kids[$count - 1] : null;
        if ($this->isBuilding()) {
            return;
        }
        $this->childNodes = new \Dom__NodeList($this->kids);
        $this->afterRelink();
        $this->refreshText();
        // the document's head / body / documentElement follow the html element's own children
        if ($this instanceof \Dom__Element && $this->parentNode instanceof \Dom__Document) {
            $this->parentNode->afterRelink();
        }
    }

    
    public function finishBuild(): void
    {
        foreach ($this->kids as $kid) {
            $kid->finishBuild();
        }
        $this->childNodes = new \Dom__NodeList($this->kids);
        $this->afterRelink();
        $this->computeText();
    }

    public function afterRelink(): void
    {
    }

    
    public function refreshText(): void
    {
        if ($this->isBuilding()) {
            return;
        }
        $walk = $this;
        while ($walk !== null) {
            $walk->computeText();
            $walk = $walk->parentNode;
        }
    }

    public function computeText(): void
    {
        $text = '';
        foreach ($this->kids as $kid) {
            if ($kid instanceof \Dom__Text) {
                $text .= $kid->data;
            } elseif ($kid instanceof \Dom__Element) {
                $text .= $kid->textContent;
            }
        }
        $this->textContent = $text;
    }
}

class Dom__CharacterData extends \Dom__Node
{
    public string $data = '';
    public int $length = 0;
    public ?\Dom__Element $previousElementSibling = null;
    public ?\Dom__Element $nextElementSibling = null;

    public function setData(string $data): void
    {
        $this->data = $data;
        $this->nodeValue = $data;
        $this->length = strlen($data);
        $this->textContent = $data;
    }

    public function computeText(): void
    {
        $this->textContent = $this->data;
    }
}

class Dom__Text extends \Dom__CharacterData
{
    public string $wholeText = '';

    public function __construct(string $data = '')
    {
        parent::__construct();
        $this->nodeType = 3;
        $this->nodeName = '#text';
        $this->setData($data);
        $this->wholeText = $data;
    }
}

class Dom__Comment extends \Dom__CharacterData
{
    public function __construct(string $data = '')
    {
        parent::__construct();
        $this->nodeType = 8;
        $this->nodeName = '#comment';
        $this->setData($data);
    }
}

class Dom__DocumentType extends \Dom__Node
{
    public string $name = '';
    public string $publicId = '';
    public string $systemId = '';

    public function __construct(string $name = '', string $publicId = '', string $systemId = '')
    {
        parent::__construct();
        $this->nodeType = 10;
        $this->nodeName = $name;
        $this->name = $name;
        $this->publicId = $publicId;
        $this->systemId = $systemId;
    }
}

class Dom__Attr extends \Dom__Node
{
    public ?string $namespaceURI = null;
    public ?string $prefix = null;
    public string $localName = '';
    public string $name = '';
    public string $value = '';
    public ?\Dom__Element $ownerElement = null;
    public bool $specified = true;

    public function __construct(string $name = '', string $value = '')
    {
        parent::__construct();
        $this->nodeType = 2;
        $this->nodeName = $name;
        $this->localName = $name;
        $this->name = $name;
        $this->value = $value;
        $this->nodeValue = $value;
        $this->textContent = $value;
    }

    public function computeText(): void
    {
        $this->textContent = $this->value;
    }
}

class Dom__Element extends \Dom__Node
{
    public ?string $namespaceURI = 'http://www.w3.org/1999/xhtml';
    public ?string $prefix = null;
    public string $localName = '';
    public string $tagName = '';
    public string $id = '';
    public string $className = '';
    public \Dom__NamedNodeMap $attributes;
    public \Dom__HTMLCollection $children;
    public ?\Dom__Element $firstElementChild = null;
    public ?\Dom__Element $lastElementChild = null;
    public int $childElementCount = 0;
    public ?\Dom__Element $previousElementSibling = null;
    public ?\Dom__Element $nextElementSibling = null;

    public function __construct(string $qualifiedName = '')
    {
        parent::__construct();
        $this->nodeType = 1;
        $name = strtolower($qualifiedName);
        $this->localName = $name;
        $this->tagName = strtoupper($qualifiedName);
        $this->nodeName = $this->tagName;
        $this->attributes = new \Dom__NamedNodeMap([]);
        $this->children = new \Dom__HTMLCollection([]);
    }

    public function afterRelink(): void
    {
        $elements = [];
        foreach ($this->kids as $kid) {
            if ($kid instanceof \Dom__Element) {
                $elements[] = $kid;
            }
        }
        $count = count($elements);
        for ($i = 0; $i < $count; $i++) {
            $elements[$i]->previousElementSibling = $i > 0 ? $elements[$i - 1] : null;
            $elements[$i]->nextElementSibling = $i < $count - 1 ? $elements[$i + 1] : null;
        }
        $this->children = new \Dom__HTMLCollection($elements);
        $this->firstElementChild = $count > 0 ? $elements[0] : null;
        $this->lastElementChild = $count > 0 ? $elements[$count - 1] : null;
        $this->childElementCount = $count;
    }

    public function hasAttributes(): bool
    {
        return $this->attributes->length > 0;
    }

    public function getAttributeNames(): array
    {
        $names = [];
        foreach ($this->attributes->items as $attr) {
            $names[] = $attr->name;
        }
        return $names;
    }

    public function getAttribute(string $qualifiedName): ?string
    {
        $attr = $this->attributes->getNamedItem($qualifiedName);
        return $attr === null ? null : $attr->value;
    }

    public function hasAttribute(string $qualifiedName): bool
    {
        return $this->attributes->getNamedItem($qualifiedName) !== null;
    }

    public function getAttributeNode(string $qualifiedName): ?\Dom__Attr
    {
        return $this->attributes->getNamedItem($qualifiedName);
    }

    public function setAttribute(string $qualifiedName, string $value): void
    {
        // HTML elements lower-case the name; svg / math elements keep it (viewBox)
        $name = $this->namespaceURI === 'http://www.w3.org/1999/xhtml' ? strtolower($qualifiedName) : $qualifiedName;
        $items = $this->attributes->items;
        foreach ($items as $attr) {
            if ($attr->name === $name) {
                $attr->value = $value;
                $attr->nodeValue = $value;
                $attr->textContent = $value;
                $this->noteAttribute($name, $value);
                return;
            }
        }
        $attr = new \Dom__Attr($name, $value);
        $attr->ownerElement = $this;
        $attr->ownerDocument = $this->ownerDocument;
        $items[] = $attr;
        $this->attributes = new \Dom__NamedNodeMap($items);
        $this->noteAttribute($name, $value);
    }

    public function removeAttribute(string $qualifiedName): void
    {
        $name = $this->namespaceURI === 'http://www.w3.org/1999/xhtml' ? strtolower($qualifiedName) : $qualifiedName;
        $items = [];
        foreach ($this->attributes->items as $attr) {
            if ($attr->name !== $name) {
                $items[] = $attr;
            }
        }
        $this->attributes = new \Dom__NamedNodeMap($items);
        if ($name === 'id') {
            $this->id = '';
        } elseif ($name === 'class') {
            $this->className = '';
        }
    }

    private function noteAttribute(string $name, string $value): void
    {
        if ($name === 'id') {
            $this->id = $value;
        } elseif ($name === 'class') {
            $this->className = $value;
        }
    }

    public function getElementsByTagName(string $qualifiedName): \Dom__HTMLCollection
    {
        $found = [];
        $this->collectByTagName(strtolower($qualifiedName), $found);
        return new \Dom__HTMLCollection($found);
    }

    public function collectByTagName(string $wanted, array &$found): void
    {
        foreach ($this->kids as $kid) {
            if ($kid instanceof \Dom__Element) {
                if ($wanted === '*' || $kid->localName === $wanted) {
                    $found[] = $kid;
                }
                $kid->collectByTagName($wanted, $found);
            }
        }
    }
}

class Dom__DocumentFragment extends \Dom__Node
{
    public function __construct()
    {
        parent::__construct();
        $this->nodeType = 11;
        $this->nodeName = '#document-fragment';
    }
}

class Dom__Document extends \Dom__Node
{
    public bool $building = false;
    public ?\Dom__DocumentType $doctype = null;
    public ?\Dom__Element $documentElement = null;
    public string $URL = 'about:blank';
    public string $documentURI = 'about:blank';
    public string $characterSet = 'UTF-8';
    public string $charset = 'UTF-8';
    public string $inputEncoding = 'UTF-8';
    public string $contentType = 'text/html';
    public ?\Dom__Element $body = null;
    public ?\Dom__Element $head = null;
    public string $title = '';

    public function __construct()
    {
        parent::__construct();
        $this->nodeType = 9;
        $this->nodeName = '#document';
    }

    public function afterRelink(): void
    {
        $this->doctype = null;
        $this->documentElement = null;
        foreach ($this->kids as $kid) {
            if ($kid instanceof \Dom__DocumentType && $this->doctype === null) {
                $this->doctype = $kid;
            } elseif ($kid instanceof \Dom__Element && $this->documentElement === null) {
                $this->documentElement = $kid;
            }
        }
        $this->head = null;
        $this->body = null;
        if ($this->documentElement !== null && $this->documentElement->localName === 'html') {
            foreach ($this->documentElement->kids as $kid) {
                if ($kid instanceof \Dom__Element) {
                    if ($kid->localName === 'head' && $this->head === null) {
                        $this->head = $kid;
                    } elseif (($kid->localName === 'body' || $kid->localName === 'frameset') && $this->body === null) {
                        $this->body = $kid;
                    }
                }
            }
        }
    }

    public function computeText(): void
    {
        $this->textContent = null;
    }

    public function getElementsByTagName(string $qualifiedName): \Dom__HTMLCollection
    {
        $found = [];
        $wanted = strtolower($qualifiedName);
        if ($this->documentElement !== null) {
            if ($wanted === '*' || $this->documentElement->localName === $wanted) {
                $found[] = $this->documentElement;
            }
            $this->documentElement->collectByTagName($wanted, $found);
        }
        return new \Dom__HTMLCollection($found);
    }

    public function createElement(string $localName): \Dom__Element
    {
        $element = new \Dom__Element($localName);
        $element->ownerDocument = $this;
        return $element;
    }

    public function createTextNode(string $data): \Dom__Text
    {
        $text = new \Dom__Text($data);
        $text->ownerDocument = $this;
        return $text;
    }

    public function createComment(string $data): \Dom__Comment
    {
        $comment = new \Dom__Comment($data);
        $comment->ownerDocument = $this;
        return $comment;
    }

    public function getElementById(string $elementId): ?\Dom__Element
    {
        if ($elementId === '' || $this->documentElement === null) {
            return null;
        }
        $found = [];
        $this->documentElement->collectByTagName('*', $found);
        if ($this->documentElement->id === $elementId) {
            return $this->documentElement;
        }
        foreach ($found as $element) {
            if ($element->id === $elementId) {
                return $element;
            }
        }
        return null;
    }
}

final class Dom__HTMLDocument extends \Dom__Document
{
    public static function createEmpty(string $encoding = 'UTF-8'): \Dom__HTMLDocument
    {
        $document = new \Dom__HTMLDocument();
        return $document;
    }

    public static function createFromString(string $source, int $options = 0, ?string $overrideEncoding = null): \Dom__HTMLDocument
    {
        $document = new \Dom__HTMLDocument();
        $document->nodeType = 13;
        $document->building = true;
        $parser = new \Dom____ElephcHtmlParser($document, $source);
        $parser->run();
        $document->building = false;
        $document->finishBuild();
        return $document;
    }
}


final class Dom____ElephcHtmlParser
{
    private \Dom__Document $doc;
    private string $src = '';
    private int $pos = 0;
    private int $len = 0;

    private array $entityTable = [];
    private array $legacyEntities = [];
    private bool $entitiesLoaded = false;

    // tokenizer state
    private string $rawTag = '';
    private string $rawMode = '';

    // tree builder state
    private string $mode = 'initial';
    private string $origMode = '';
    private array $open = [];
    private array $afe = [];
    private ?\Dom__Element $headEl = null;
    private ?\Dom__Element $formEl = null;
    private bool $framesetOk = true;
    private bool $quirks = true;
    private bool $foster = false;
    private bool $stop = false;
    private string $pendingText = '';
    private array $tableTexts = [];
    private array $templateModes = [];

    public function __construct(\Dom__Document $doc, string $source)
    {
        $this->doc = $doc;
        $source = str_replace("\r\n", "\n", $source);
        $source = str_replace("\r", "\n", $source);
        $this->src = $source;
        $this->len = strlen($source);
    }

    // ------------------------------------------------------------------------------------------
    // Tokenizer
    // ------------------------------------------------------------------------------------------

    public function run(): void
    {
        while (!$this->stop && $this->pos < $this->len) {
            if ($this->rawTag !== '') {
                $this->scanRaw();
                continue;
            }
            $lt = strpos($this->src, '<', $this->pos);
            $amp = strpos($this->src, '&', $this->pos);
            $next = $this->len;
            if ($lt !== false) {
                $next = $lt;
            }
            if ($amp !== false && $amp < $next) {
                $next = $amp;
            }
            if ($next > $this->pos) {
                $this->emitChars(substr($this->src, $this->pos, $next - $this->pos));
                $this->pos = $next;
                continue;
            }
            if ($this->src[$this->pos] === '&') {
                $this->emitChars($this->charRef(false));
                continue;
            }
            $this->tagOpen();
        }
        $this->flushText();
        if (!$this->stop) {
            $this->tbEof();
        }
    }

    private function peek(int $offset = 0): string
    {
        $at = $this->pos + $offset;
        if ($at >= $this->len) {
            return '';
        }
        return $this->src[$at];
    }

    private static function isSpace(string $c): bool
    {
        return $c === ' ' || $c === "\t" || $c === "\n" || $c === "\f";
    }

    private static function isAlpha(string $c): bool
    {
        return ($c >= 'a' && $c <= 'z') || ($c >= 'A' && $c <= 'Z');
    }

    private function tagOpen(): void
    {
        // $this->src[$this->pos] === '<'
        $c = $this->peek(1);
        if ($c === '') {
            $this->pos++;
            $this->emitChars('<');
            return;
        }
        if ($c === '!') {
            $this->markupDeclaration();
            return;
        }
        if ($c === '/') {
            $d = $this->peek(2);
            if ($d === '') {
                $this->pos += 2;
                $this->emitChars('</');
                return;
            }
            if (self::isAlpha($d)) {
                $this->pos += 2;
                $this->readTag(false);
                return;
            }
            if ($d === '>') {
                $this->pos += 3;
                return;
            }
            $this->pos += 2;
            $this->bogusComment('');
            return;
        }
        if (self::isAlpha($c)) {
            $this->pos += 1;
            $this->readTag(true);
            return;
        }
        if ($c === '?') {
            $this->pos += 1;
            $this->bogusComment('');
            return;
        }
        $this->pos++;
        $this->emitChars('<');
    }

    private function bogusComment(string $prefix): void
    {
        $end = strpos($this->src, '>', $this->pos);
        if ($end === false) {
            $text = $prefix . substr($this->src, $this->pos);
            $this->pos = $this->len;
        } else {
            $text = $prefix . substr($this->src, $this->pos, $end - $this->pos);
            $this->pos = $end + 1;
        }
        $this->emitComment(str_replace("\0", "\u{fffd}", $text));
    }

    private function markupDeclaration(): void
    {
        // at '<!'
        if (substr($this->src, $this->pos + 2, 2) === '--') {
            $this->pos += 4;
            $this->readComment();
            return;
        }
        if (strtolower(substr($this->src, $this->pos + 2, 7)) === 'doctype') {
            $this->pos += 9;
            $this->readDoctype();
            return;
        }
        if (substr($this->src, $this->pos + 2, 7) === '[CDATA[') {
            $this->pos += 2;
            $this->bogusComment('');
            return;
        }
        $this->pos += 2;
        $this->bogusComment('');
    }

    private function readComment(): void
    {
        // after '<!--'
        if ($this->peek() === '>') {
            $this->pos++;
            $this->emitComment('');
            return;
        }
        if ($this->peek() === '-' && $this->peek(1) === '>') {
            $this->pos += 2;
            $this->emitComment('');
            return;
        }
        $start = $this->pos;
        $i = $this->pos;
        while ($i < $this->len) {
            $end = strpos($this->src, '--', $i);
            if ($end === false) {
                break;
            }
            $after = $end + 2;
            $c = $after < $this->len ? $this->src[$after] : '';
            if ($c === '>') {
                $this->pos = $after + 1;
                $this->emitComment(str_replace("\0", "\u{fffd}", substr($this->src, $start, $end - $start)));
                return;
            }
            if ($c === '!' && $after + 1 < $this->len && $this->src[$after + 1] === '>') {
                $this->pos = $after + 2;
                $this->emitComment(str_replace("\0", "\u{fffd}", substr($this->src, $start, $end - $start)));
                return;
            }
            if ($c === '-') {
                $i = $end + 1;
                continue;
            }
            $i = $end + 1;
        }
        // end of input inside the comment: the comment is emitted with what was read
        $text = substr($this->src, $start);
        $this->pos = $this->len;
        if (substr($text, -2) === '--') {
            $text = substr($text, 0, -2);
        } elseif (substr($text, -1) === '-') {
            $text = substr($text, 0, -1);
        }
        $this->emitComment(str_replace("\0", "\u{fffd}", $text));
    }

    private function readDoctype(): void
    {
        // after '<!DOCTYPE'
        $name = '';
        $public = null;
        $system = null;
        $quirks = false;
        $this->skipSpaces();
        if ($this->pos >= $this->len) {
            $this->emitDoctype('', null, null, true);
            return;
        }
        if ($this->peek() === '>') {
            $this->pos++;
            $this->emitDoctype('', null, null, true);
            return;
        }
        while ($this->pos < $this->len) {
            $c = $this->src[$this->pos];
            if (self::isSpace($c) || $c === '>') {
                break;
            }
            $name .= $c;
            $this->pos++;
        }
        $name = strtolower(str_replace("\0", "\u{fffd}", $name));
        $this->skipSpaces();
        if ($this->pos >= $this->len) {
            $this->emitDoctype($name, null, null, true);
            return;
        }
        if ($this->peek() !== '>') {
            $word = strtolower(substr($this->src, $this->pos, 6));
            if ($word === 'public') {
                $this->pos += 6;
                $public = $this->doctypeId();
                if ($public !== null) {
                    $system = $this->doctypeId();
                }
            } elseif ($word === 'system') {
                $this->pos += 6;
                $system = $this->doctypeId();
            } else {
                $quirks = true;
            }
        }
        // skip to '>'
        $end = strpos($this->src, '>', $this->pos);
        if ($end === false) {
            $this->pos = $this->len;
            $quirks = true;
        } else {
            $this->pos = $end + 1;
        }
        $this->emitDoctype($name, $public, $system, $quirks);
    }

    private function doctypeId(): ?string
    {
        $this->skipSpaces();
        $quote = $this->peek();
        if ($quote !== '"' && $quote !== "'") {
            return null;
        }
        $this->pos++;
        $end = strpos($this->src, $quote, $this->pos);
        if ($end === false) {
            $value = substr($this->src, $this->pos);
            $this->pos = $this->len;
            return $value;
        }
        $value = substr($this->src, $this->pos, $end - $this->pos);
        $this->pos = $end + 1;
        return $value;
    }

    private function skipSpaces(): void
    {
        while ($this->pos < $this->len && self::isSpace($this->src[$this->pos])) {
            $this->pos++;
        }
    }

    private function readTag(bool $isStart): void
    {
        // $this->pos is on the first letter of the tag name
        $name = '';
        while ($this->pos < $this->len) {
            $c = $this->src[$this->pos];
            if (self::isSpace($c) || $c === '/' || $c === '>') {
                break;
            }
            $name .= $c;
            $this->pos++;
        }
        $name = strtolower(str_replace("\0", "\u{fffd}", $name));
        $attrs = [];
        $seen = [];
        $selfClosing = false;
        while (true) {
            $this->skipSpaces();
            if ($this->pos >= $this->len) {
                return; // eof in tag: the token is dropped
            }
            $c = $this->src[$this->pos];
            if ($c === '>') {
                $this->pos++;
                break;
            }
            if ($c === '/') {
                $this->pos++;
                if ($this->peek() === '>') {
                    $this->pos++;
                    $selfClosing = true;
                    break;
                }
                continue;
            }
            // attribute name
            $attrName = $c;
            $this->pos++;
            while ($this->pos < $this->len) {
                $d = $this->src[$this->pos];
                if (self::isSpace($d) || $d === '/' || $d === '>' || $d === '=') {
                    break;
                }
                $attrName .= $d;
                $this->pos++;
            }
            $attrName = strtolower(str_replace("\0", "\u{fffd}", $attrName));
            $this->skipSpaces();
            $value = '';
            if ($this->peek() === '=') {
                $this->pos++;
                $this->skipSpaces();
                $q = $this->peek();
                if ($q === '"' || $q === "'") {
                    $this->pos++;
                    $value = $this->attrValueQuoted($q);
                    if ($this->pos >= $this->len && $this->unterminated) {
                        return;
                    }
                } else {
                    $value = $this->attrValueUnquoted();
                }
            }
            if (!isset($seen[$attrName])) {
                $seen[$attrName] = true;
                $attrs[] = [$attrName, $value];
            }
        }
        if ($isStart) {
            $this->tbStart($name, $attrs, $selfClosing);
        } else {
            $this->tbEnd($name);
        }
    }

    private bool $unterminated = false;

    private function attrValueQuoted(string $quote): string
    {
        $value = '';
        $this->unterminated = false;
        while ($this->pos < $this->len) {
            $c = $this->src[$this->pos];
            if ($c === $quote) {
                $this->pos++;
                return $value;
            }
            if ($c === '&') {
                $value .= $this->charRef(true);
                continue;
            }
            $stop = strpos($this->src, $quote, $this->pos);
            $amp = strpos($this->src, '&', $this->pos);
            $end = $stop === false ? $this->len : $stop;
            if ($amp !== false && $amp < $end) {
                $end = $amp;
            }
            $value .= str_replace("\0", "\u{fffd}", substr($this->src, $this->pos, $end - $this->pos));
            $this->pos = $end;
        }
        $this->unterminated = true;
        return $value;
    }

    private function attrValueUnquoted(): string
    {
        $value = '';
        while ($this->pos < $this->len) {
            $c = $this->src[$this->pos];
            if (self::isSpace($c) || $c === '>') {
                break;
            }
            if ($c === '&') {
                $value .= $this->charRef(true);
                continue;
            }
            $value .= $c === "\0" ? "\u{fffd}" : $c;
            $this->pos++;
        }
        return $value;
    }

    
    private function charRef(bool $inAttribute): string
    {
        $this->pos++; // '&'
        $c = $this->peek();
        if ($c === '#') {
            $hex = false;
            $i = $this->pos + 1;
            if ($i < $this->len && ($this->src[$i] === 'x' || $this->src[$i] === 'X')) {
                $hex = true;
                $i++;
            }
            $digitsStart = $i;
            while ($i < $this->len) {
                $d = $this->src[$i];
                if (($d >= '0' && $d <= '9') || ($hex && (($d >= 'a' && $d <= 'f') || ($d >= 'A' && $d <= 'F')))) {
                    $i++;
                } else {
                    break;
                }
            }
            if ($i === $digitsStart) {
                return '&';
            }
            $digits = substr($this->src, $digitsStart, $i - $digitsStart);
            if ($i < $this->len && $this->src[$i] === ';') {
                $i++;
            }
            $this->pos = $i;
            if (strlen($digits) > 8) {
                return "\u{fffd}";
            }
            $code = 0;
            if ($hex) {
                $code = (int) hexdec($digits);
            } else {
                $code = (int) $digits;
            }
            return self::codepointText($code);
        }
        if (!self::isAlnum($c)) {
            return '&';
        }
        $i = $this->pos;
        $name = '';
        while ($i < $this->len && self::isAlnum($this->src[$i]) && strlen($name) < 40) {
            $name .= $this->src[$i];
            $i++;
        }
        if (!$this->entitiesLoaded) {
            $this->entityTable = __elephc_dom_entity_table();
            $this->legacyEntities = __elephc_dom_legacy_entities();
            $this->entitiesLoaded = true;
        }
        if ($i < $this->len && $this->src[$i] === ';' && isset($this->entityTable[$name . ';'])) {
            $this->pos = $i + 1;
            return $this->entityTable[$name . ';'];
        }
        // legacy names match without the semicolon, longest first
        for ($n = strlen($name); $n >= 2; $n--) {
            $try = substr($name, 0, $n);
            if (in_array($try, $this->legacyEntities, true)) {
                $after = $this->pos + $n;
                $nextChar = $after < $this->len ? $this->src[$after] : '';
                if ($inAttribute && ($nextChar === '=' || self::isAlnum($nextChar))) {
                    return '&';
                }
                $this->pos = $after;
                return $this->entityTable[$try . ';'];
            }
        }
        return '&';
    }

    private static function isAlnum(string $c): bool
    {
        if ($c === '') {
            return false;
        }
        return ($c >= '0' && $c <= '9') || ($c >= 'a' && $c <= 'z') || ($c >= 'A' && $c <= 'Z');
    }

    private static function codepointText(int $code): string
    {
        if ($code === 0 || $code > 0x10FFFF || ($code >= 0xD800 && $code <= 0xDFFF)) {
            return "\u{fffd}";
        }
        if ($code >= 0x80 && $code <= 0x9F) {
            $map = [
                0x80 => 0x20AC, 0x82 => 0x201A, 0x83 => 0x0192, 0x84 => 0x201E, 0x85 => 0x2026,
                0x86 => 0x2020, 0x87 => 0x2021, 0x88 => 0x02C6, 0x89 => 0x2030, 0x8A => 0x0160,
                0x8B => 0x2039, 0x8C => 0x0152, 0x8E => 0x017D, 0x91 => 0x2018, 0x92 => 0x2019,
                0x93 => 0x201C, 0x94 => 0x201D, 0x95 => 0x2022, 0x96 => 0x2013, 0x97 => 0x2014,
                0x98 => 0x02DC, 0x99 => 0x2122, 0x9A => 0x0161, 0x9B => 0x203A, 0x9C => 0x0153,
                0x9E => 0x017E, 0x9F => 0x0178,
            ];
            if (isset($map[$code])) {
                $code = (int) $map[$code];
            }
        }
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

    
    private function scanRaw(): void
    {
        $tag = $this->rawTag;
        $mode = $this->rawMode;
        if ($mode === 'plaintext') {
            $this->emitChars(substr($this->src, $this->pos));
            $this->pos = $this->len;
            return;
        }
        $from = $this->pos;
        $limit = $this->len;
        $found = -1;
        $i = $from;
        while (true) {
            $at = stripos($this->src, '</' . $tag, $i);
            if ($at === false) {
                break;
            }
            $after = $at + 2 + strlen($tag);
            $c = $after < $this->len ? $this->src[$after] : '';
            if ($c === '' || self::isSpace($c) || $c === '/' || $c === '>') {
                $found = $at;
                break;
            }
            $i = $at + 1;
        }
        $end = $found === -1 ? $this->len : $found;
        $text = substr($this->src, $from, $end - $from);
        if ($text !== '') {
            if ($mode === 'rcdata') {
                $this->emitChars($this->decodeInText($text));
            } else {
                $this->emitChars(str_replace("\0", "\u{fffd}", $text));
            }
        }
        $this->pos = $end;
        if ($found === -1) {
            $this->rawTag = '';
            return;
        }
        $this->rawTag = '';
        $this->pos = $found + 2;
        $this->readTag(false);
    }

    private function decodeInText(string $text): string
    {
        if (strpos($text, '&') === false) {
            return $text;
        }
        $savedSrc = $this->src;
        $savedPos = $this->pos;
        $savedLen = $this->len;
        $this->src = $text;
        $this->len = strlen($text);
        $this->pos = 0;
        $out = '';
        while ($this->pos < $this->len) {
            $amp = strpos($this->src, '&', $this->pos);
            if ($amp === false) {
                $out .= substr($this->src, $this->pos);
                break;
            }
            $out .= substr($this->src, $this->pos, $amp - $this->pos);
            $this->pos = $amp;
            $out .= $this->charRef(false);
        }
        $this->src = $savedSrc;
        $this->pos = $savedPos;
        $this->len = $savedLen;
        return $out;
    }

    // ------------------------------------------------------------------------------------------
    // Token entry points (feed the tree builder)
    // ------------------------------------------------------------------------------------------

    private function emitChars(string $text): void
    {
        if ($text === '') {
            return;
        }
        $this->tbChars($text);
    }

    private function emitComment(string $text): void
    {
        $this->flushText();
        $this->tbComment($text);
    }

    private function emitDoctype(string $name, ?string $public, ?string $system, bool $forceQuirks): void
    {
        $this->flushText();
        $this->tbDoctype($name, $public, $system, $forceQuirks);
    }

    // ------------------------------------------------------------------------------------------
    // Tree builder
    // ------------------------------------------------------------------------------------------

    private function curr(): ?\Dom__Element
    {
        $n = count($this->open);
        return $n > 0 ? $this->open[$n - 1] : null;
    }

    private function currName(): string
    {
        $c = $this->curr();
        return $c === null ? '' : $c->localName;
    }

    private function pushOpen(\Dom__Element $el): void
    {
        $this->open[] = $el;
    }

    private function popOpen(): ?\Dom__Element
    {
        if (count($this->open) === 0) {
            return null;
        }
        return array_pop($this->open);
    }

    private function stackHas(string $name): bool
    {
        foreach ($this->open as $el) {
            if ($el->localName === $name) {
                return true;
            }
        }
        return false;
    }

    private function openIndex(\Dom__Element $el): int
    {
        for ($i = count($this->open) - 1; $i >= 0; $i--) {
            if ($this->open[$i] === $el) {
                return $i;
            }
        }
        return -1;
    }

    private function removeOpen(\Dom__Element $el): void
    {
        $i = $this->openIndex($el);
        if ($i >= 0) {
            array_splice($this->open, $i, 1);
        }
    }

    private function makeElement(string $name, array $attrs, string $ns = ''): \Dom__Element
    {
        $el = new \Dom__Element($name);
        $el->ownerDocument = $this->doc;
        if ($ns !== '') {
            $el->namespaceURI = $ns;
            $el->localName = $name;
            $el->tagName = $name;
            $el->nodeName = $name;
        }
        foreach ($attrs as $pair) {
            $el->setAttribute($pair[0], $pair[1]);
        }
        return $el;
    }

    public const SVG = 'http://www.w3.org/2000/svg';
    public const MATHML = 'http://www.w3.org/1998/Math/MathML';
    public const HTMLNS = 'http://www.w3.org/1999/xhtml';

    private function insertForeign(string $name, array $attrs, string $ns): \Dom__Element
    {
        if ($ns === self::SVG) {
            $name = self::svgTagName($name);
            $attrs = self::adjustSvgAttributes($attrs);
        } else {
            $attrs = self::adjustMathAttributes($attrs);
        }
        $el = $this->makeElement($name, $attrs, $ns);
        $this->insertNode($el);
        $this->pushOpen($el);
        return $el;
    }

    private static function svgTagName(string $name): string
    {
        $map = [
            'altglyph' => 'altGlyph', 'altglyphdef' => 'altGlyphDef', 'altglyphitem' => 'altGlyphItem',
            'animatecolor' => 'animateColor', 'animatemotion' => 'animateMotion', 'animatetransform' => 'animateTransform',
            'clippath' => 'clipPath', 'feblend' => 'feBlend', 'fecolormatrix' => 'feColorMatrix',
            'fecomponenttransfer' => 'feComponentTransfer', 'fecomposite' => 'feComposite',
            'feconvolvematrix' => 'feConvolveMatrix', 'fediffuselighting' => 'feDiffuseLighting',
            'fedisplacementmap' => 'feDisplacementMap', 'fedistantlight' => 'feDistantLight',
            'fedropshadow' => 'feDropShadow', 'feflood' => 'feFlood', 'fefunca' => 'feFuncA', 'fefuncb' => 'feFuncB',
            'fefuncg' => 'feFuncG', 'fefuncr' => 'feFuncR', 'fegaussianblur' => 'feGaussianBlur', 'feimage' => 'feImage',
            'femerge' => 'feMerge', 'femergenode' => 'feMergeNode', 'femorphology' => 'feMorphology',
            'feoffset' => 'feOffset', 'fepointlight' => 'fePointLight', 'fespecularlighting' => 'feSpecularLighting',
            'fespotlight' => 'feSpotLight', 'fetile' => 'feTile', 'feturbulence' => 'feTurbulence',
            'foreignobject' => 'foreignObject', 'glyphref' => 'glyphRef', 'lineargradient' => 'linearGradient',
            'radialgradient' => 'radialGradient', 'textpath' => 'textPath',
        ];
        return $map[$name] ?? $name;
    }

    private static function adjustSvgAttributes(array $attrs): array
    {
        $map = [
            'attributename' => 'attributeName', 'attributetype' => 'attributeType', 'basefrequency' => 'baseFrequency',
            'baseprofile' => 'baseProfile', 'calcmode' => 'calcMode', 'clippathunits' => 'clipPathUnits',
            'diffuseconstant' => 'diffuseConstant', 'edgemode' => 'edgeMode', 'filterunits' => 'filterUnits',
            'glyphref' => 'glyphRef', 'gradienttransform' => 'gradientTransform', 'gradientunits' => 'gradientUnits',
            'kernelmatrix' => 'kernelMatrix', 'kernelunitlength' => 'kernelUnitLength', 'keypoints' => 'keyPoints',
            'keysplines' => 'keySplines', 'keytimes' => 'keyTimes', 'lengthadjust' => 'lengthAdjust',
            'limitingconeangle' => 'limitingConeAngle', 'markerheight' => 'markerHeight', 'markerunits' => 'markerUnits',
            'markerwidth' => 'markerWidth', 'maskcontentunits' => 'maskContentUnits', 'maskunits' => 'maskUnits',
            'numoctaves' => 'numOctaves', 'pathlength' => 'pathLength', 'patterncontentunits' => 'patternContentUnits',
            'patterntransform' => 'patternTransform', 'patternunits' => 'patternUnits', 'pointsatx' => 'pointsAtX',
            'pointsaty' => 'pointsAtY', 'pointsatz' => 'pointsAtZ', 'preservealpha' => 'preserveAlpha',
            'preserveaspectratio' => 'preserveAspectRatio', 'primitiveunits' => 'primitiveUnits', 'refx' => 'refX',
            'refy' => 'refY', 'repeatcount' => 'repeatCount', 'repeatdur' => 'repeatDur',
            'requiredextensions' => 'requiredExtensions', 'requiredfeatures' => 'requiredFeatures',
            'specularconstant' => 'specularConstant', 'specularexponent' => 'specularExponent',
            'spreadmethod' => 'spreadMethod', 'startoffset' => 'startOffset', 'stddeviation' => 'stdDeviation',
            'stitchtiles' => 'stitchTiles', 'surfacescale' => 'surfaceScale', 'systemlanguage' => 'systemLanguage',
            'tablevalues' => 'tableValues', 'targetx' => 'targetX', 'targety' => 'targetY', 'textlength' => 'textLength',
            'viewbox' => 'viewBox', 'viewtarget' => 'viewTarget', 'xchannelselector' => 'xChannelSelector',
            'ychannelselector' => 'yChannelSelector', 'zoomandpan' => 'zoomAndPan',
        ];
        $out = [];
        foreach ($attrs as $pair) {
            $name = $map[$pair[0]] ?? $pair[0];
            $out[] = [$name, $pair[1]];
        }
        return $out;
    }

    private static function adjustMathAttributes(array $attrs): array
    {
        $out = [];
        foreach ($attrs as $pair) {
            $name = $pair[0] === 'definitionurl' ? 'definitionURL' : $pair[0];
            $out[] = [$name, $pair[1]];
        }
        return $out;
    }

    
    private function inForeign(string $kind, string $name): bool
    {
        $node = $this->curr();
        if ($node === null || $node->namespaceURI === self::HTMLNS) {
            return false;
        }
        $isMath = $node->namespaceURI === self::MATHML;
        if ($isMath && in_array($node->localName, ['mi', 'mo', 'mn', 'ms', 'mtext'], true)) {
            if ($kind === 'start' && $name !== 'mglyph' && $name !== 'malignmark') {
                return false;
            }
            if ($kind === 'chars') {
                return false;
            }
        }
        if ($isMath && $node->localName === 'annotation-xml' && $kind === 'start' && $name === 'svg') {
            return false;
        }
        if ($this->isHtmlIntegration($node) && ($kind === 'start' || $kind === 'chars')) {
            return false;
        }
        return true;
    }

    private function isHtmlIntegration(\Dom__Element $node): bool
    {
        if ($node->namespaceURI === self::SVG) {
            return $node->localName === 'foreignObject' || $node->localName === 'desc' || $node->localName === 'title';
        }
        if ($node->namespaceURI === self::MATHML && $node->localName === 'annotation-xml') {
            $enc = $node->getAttribute('encoding');
            if ($enc !== null) {
                $enc = strtolower($enc);
                return $enc === 'text/html' || $enc === 'application/xhtml+xml';
            }
        }
        return false;
    }

    private function foreignStart(string $name, array $attrs, bool $selfClosing): void
    {
        $breakout = ['b', 'big', 'blockquote', 'body', 'br', 'center', 'code', 'dd', 'div', 'dl', 'dt', 'em', 'embed',
            'h1', 'h2', 'h3', 'h4', 'h5', 'h6', 'head', 'hr', 'i', 'img', 'li', 'listing', 'menu', 'meta', 'nobr', 'ol',
            'p', 'pre', 'ruby', 's', 'small', 'span', 'strong', 'strike', 'sub', 'table', 'tt', 'u', 'ul', 'var'];
        $isBreakout = in_array($name, $breakout, true);
        if ($name === 'font') {
            foreach ($attrs as $pair) {
                if ($pair[0] === 'color' || $pair[0] === 'face' || $pair[0] === 'size') {
                    $isBreakout = true;
                }
            }
        }
        if ($isBreakout) {
            while (count($this->open) > 0) {
                $node = $this->curr();
                if ($node->namespaceURI === self::HTMLNS || $this->isHtmlIntegration($node)
                    || ($node->namespaceURI === self::MATHML && in_array($node->localName, ['mi', 'mo', 'mn', 'ms', 'mtext'], true))) {
                    break;
                }
                $this->popOpen();
            }
            $this->dispatchStart($name, $attrs, $selfClosing);
            return;
        }
        $ns = $this->curr()->namespaceURI;
        $el = $this->insertForeign($name, $attrs, $ns);
        if ($selfClosing) {
            $this->popOpen();
        }
    }

    private function foreignEnd(string $name): void
    {
        if ($name === 'p' || $name === 'br') {
            while (count($this->open) > 0) {
                $top = $this->curr();
                if ($top->namespaceURI === self::HTMLNS || $this->isHtmlIntegration($top)
                    || ($top->namespaceURI === self::MATHML && in_array($top->localName, ['mi', 'mo', 'mn', 'ms', 'mtext'], true))) {
                    break;
                }
                $this->popOpen();
            }
            $this->dispatchEnd($name);
            return;
        }
        $i = count($this->open) - 1;
        $node = $this->open[$i];
        if ($node->namespaceURI === self::SVG && $node->localName === 'script' && $name === 'script') {
            $this->popOpen();
            return;
        }
        while (true) {
            if ($i === 0) {
                return;
            }
            if (strtolower($node->localName) === $name) {
                while (count($this->open) > $i) {
                    array_pop($this->open);
                }
                return;
            }
            $i--;
            $node = $this->open[$i];
            if ($node->namespaceURI === self::HTMLNS) {
                $this->dispatchEnd($name);
                return;
            }
        }
    }

    
    private function insertionParent(?\Dom__Node $override, ?\Dom__Node &$before): \Dom__Node
    {
        $before = null;
        $target = $override;
        if ($target === null) {
            $target = $this->curr();
            if ($target === null) {
                $target = $this->doc;
            }
        }
        if ($this->foster && $target instanceof \Dom__Element && in_array($target->localName, ['table', 'tbody', 'tfoot', 'thead', 'tr'], true)) {
            $tableIndex = -1;
            for ($i = count($this->open) - 1; $i >= 0; $i--) {
                if ($this->open[$i]->localName === 'table') {
                    $tableIndex = $i;
                    break;
                }
            }
            if ($tableIndex < 0) {
                return $this->open[0];
            }
            $table = $this->open[$tableIndex];
            if ($table->parentNode !== null) {
                $before = $table;
                return $table->parentNode;
            }
            return $tableIndex > 0 ? $this->open[$tableIndex - 1] : $this->doc;
        }
        return $target;
    }

    private function insertNode(\Dom__Node $node, ?\Dom__Node $override = null): void
    {
        $before = null;
        $parent = $this->insertionParent($override, $before);
        $parent->insertBefore($node, $before);
    }

    private function insertElement(string $name, array $attrs): \Dom__Element
    {
        $el = $this->makeElement($name, $attrs);
        $this->insertNode($el);
        $this->pushOpen($el);
        return $el;
    }

    private function insertVoid(string $name, array $attrs): \Dom__Element
    {
        $el = $this->makeElement($name, $attrs);
        $this->insertNode($el);
        return $el;
    }

    private string $tableTextBuf = '';

    
    private function resolveTableText(): void
    {
        $text = $this->tableTextBuf;
        $this->tableTextBuf = '';
        if ($text === '') {
            return;
        }
        if (trim($text, " \t\n\f") === '') {
            $this->insertCharsNow($text);
            return;
        }
        $saved = $this->foster;
        $this->foster = true;
        $this->flushText();
        $this->bodyChars($text);
        $this->flushText();
        $this->foster = $saved;
    }

    private function flushText(): void
    {
        if ($this->tableTextBuf !== '') {
            $this->resolveTableText();
        }
        if ($this->pendingText === '') {
            return;
        }
        $text = $this->pendingText;
        $this->pendingText = '';
        $before = null;
        $parent = $this->insertionParent(null, $before);
        if ($before === null) {
            $last = $parent->lastChild;
        } else {
            $last = $before->previousSibling;
        }
        if ($last instanceof \Dom__Text) {
            $last->setData($last->data . $text);
            $last->wholeText = $last->data;
            $last->refreshText();
            return;
        }
        $node = new \Dom__Text($text);
        $node->ownerDocument = $this->doc;
        $parent->insertBefore($node, $before);
    }

    private function insertCharsNow(string $text): void
    {
        $this->pendingText .= $text;
    }

    // ---- scope ----------------------------------------------------------------------------

    private function inScope(string $name, string $kind = 'default'): bool
    {
        for ($i = count($this->open) - 1; $i >= 0; $i--) {
            $el = $this->open[$i];
            $n = $el->localName;
            if ($n === $name && $el->namespaceURI === self::HTMLNS) {
                return true;
            }
            if ($this->elementIsBoundary($el, $kind)) {
                return false;
            }
        }
        return false;
    }

    
    private function elementIsBoundary(\Dom__Element $el, string $kind): bool
    {
        if ($el->namespaceURI === self::HTMLNS) {
            return $this->scopeBoundary($el->localName, $kind);
        }
        if ($kind === 'table' || $kind === 'select') {
            return $kind === 'table' ? false : true;
        }
        if ($el->namespaceURI === self::MATHML) {
            return in_array($el->localName, ['mi', 'mo', 'mn', 'ms', 'mtext', 'annotation-xml'], true);
        }
        return $el->localName === 'foreignObject' || $el->localName === 'desc' || $el->localName === 'title';
    }

    private function scopeBoundary(string $n, string $kind): bool
    {
        if ($kind === 'table') {
            return $n === 'html' || $n === 'table' || $n === 'template';
        }
        if ($kind === 'select') {
            return $n !== 'optgroup' && $n !== 'option';
        }
        if (in_array($n, ['applet', 'caption', 'html', 'table', 'td', 'th', 'marquee', 'object', 'template'], true)) {
            return true;
        }
        if ($kind === 'list' && ($n === 'ol' || $n === 'ul')) {
            return true;
        }
        if ($kind === 'button' && $n === 'button') {
            return true;
        }
        return false;
    }

    private function headingInScope(): bool
    {
        for ($i = count($this->open) - 1; $i >= 0; $i--) {
            $el = $this->open[$i];
            $n = $el->localName;
            if (strlen($n) === 2 && $n[0] === 'h' && $n[1] >= '1' && $n[1] <= '6' && $el->namespaceURI === self::HTMLNS) {
                return true;
            }
            if ($this->elementIsBoundary($el, 'default')) {
                return false;
            }
        }
        return false;
    }

    private function generateImpliedEndTags(string $except = ''): void
    {
        while (true) {
            $n = $this->currName();
            if ($n !== $except && in_array($n, ['dd', 'dt', 'li', 'optgroup', 'option', 'p', 'rb', 'rp', 'rt', 'rtc'], true)) {
                $this->popOpen();
                continue;
            }
            break;
        }
    }

    private function generateAllImpliedEndTags(): void
    {
        while (in_array($this->currName(), ['caption', 'colgroup', 'dd', 'dt', 'li', 'optgroup', 'option', 'p', 'rb', 'rp', 'rt', 'rtc', 'tbody', 'td', 'tfoot', 'th', 'thead', 'tr'], true)) {
            $this->popOpen();
        }
    }

    private function closeP(): void
    {
        $this->generateImpliedEndTags('p');
        while (count($this->open) > 0) {
            $el = $this->popOpen();
            if ($el->localName === 'p') {
                break;
            }
        }
    }

    private function closePIfInButtonScope(): void
    {
        if ($this->inScope('p', 'button')) {
            $this->closeP();
        }
    }

    private function popUntil(string $name): void
    {
        while (count($this->open) > 0) {
            $el = $this->popOpen();
            if ($el->localName === $name) {
                break;
            }
        }
    }

    private function popUntilOneOf(array $names): void
    {
        while (count($this->open) > 0) {
            $el = $this->popOpen();
            if (in_array($el->localName, $names, true)) {
                break;
            }
        }
    }

    private static function isSpecial(string $n): bool
    {
        return in_array($n, [
            'address', 'applet', 'area', 'article', 'aside', 'base', 'basefont', 'bgsound', 'blockquote',
            'body', 'br', 'button', 'caption', 'center', 'col', 'colgroup', 'dd', 'details', 'dir', 'div',
            'dl', 'dt', 'embed', 'fieldset', 'figcaption', 'figure', 'footer', 'form', 'frame', 'frameset',
            'h1', 'h2', 'h3', 'h4', 'h5', 'h6', 'head', 'header', 'hgroup', 'hr', 'html', 'iframe', 'img',
            'input', 'keygen', 'li', 'link', 'listing', 'main', 'marquee', 'menu', 'meta', 'nav', 'noembed',
            'noframes', 'noscript', 'object', 'ol', 'p', 'param', 'plaintext', 'pre', 'script', 'search',
            'section', 'select', 'source', 'style', 'summary', 'table', 'tbody', 'td', 'template', 'textarea',
            'tfoot', 'th', 'thead', 'title', 'tr', 'track', 'ul', 'wbr', 'xmp',
        ], true);
    }

    private function isSpecialEl(\Dom__Element $el): bool
    {
        if ($el->namespaceURI === self::HTMLNS) {
            return self::isSpecial($el->localName);
        }
        if ($el->namespaceURI === self::MATHML) {
            return in_array($el->localName, ['mi', 'mo', 'mn', 'ms', 'mtext', 'annotation-xml'], true);
        }
        return $el->localName === 'foreignObject' || $el->localName === 'desc' || $el->localName === 'title';
    }

    private static function isFormatting(string $n): bool
    {
        return in_array($n, ['a', 'b', 'big', 'code', 'em', 'font', 'i', 'nobr', 's', 'small', 'strike', 'strong', 'tt', 'u'], true);
    }

    // ---- active formatting elements ---------------------------------------------------------

    private function afeIndexOf(\Dom__Element $el): int
    {
        for ($i = count($this->afe) - 1; $i >= 0; $i--) {
            if ($this->afe[$i] === $el) {
                return $i;
            }
        }
        return -1;
    }

    private function afePush(\Dom__Element $el, array $attrs): void
    {
        // Noah's Ark clause: at most three identical entries after the last marker
        $count = 0;
        for ($i = count($this->afe) - 1; $i >= 0; $i--) {
            $entry = $this->afe[$i];
            if ($entry === null) {
                break;
            }
            if ($entry->localName === $el->localName && $this->sameAttributes($entry, $el)) {
                $count++;
                if ($count === 3) {
                    array_splice($this->afe, $i, 1);
                    break;
                }
            }
        }
        $this->afe[] = $el;
    }

    private function sameAttributes(\Dom__Element $a, \Dom__Element $b): bool
    {
        if ($a->attributes->length !== $b->attributes->length) {
            return false;
        }
        foreach ($a->attributes->items as $attr) {
            $other = $b->getAttribute($attr->name);
            if ($other === null || $other !== $attr->value) {
                return false;
            }
        }
        return true;
    }

    private function reconstructFormatting(): void
    {
        $n = count($this->afe);
        if ($n === 0) {
            return;
        }
        $entry = $this->afe[$n - 1];
        if ($entry === null || $this->openIndex($entry) >= 0) {
            return;
        }
        $i = $n - 1;
        while ($i > 0) {
            $i--;
            $entry = $this->afe[$i];
            if ($entry === null || $this->openIndex($entry) >= 0) {
                $i++;
                break;
            }
        }
        for (; $i < $n; $i++) {
            $old = $this->afe[$i];
            $attrs = [];
            foreach ($old->attributes->items as $attr) {
                $attrs[] = [$attr->name, $attr->value];
            }
            $el = $this->insertElement($old->localName, $attrs);
            array_splice($this->afe, $i, 1, [$el]);
        }
    }

    private function clearFormattingToMarker(): void
    {
        while (count($this->afe) > 0) {
            $entry = array_pop($this->afe);
            if ($entry === null) {
                break;
            }
        }
    }

    private function adoptionAgency(string $subject): bool
    {
        $current = $this->curr();
        if ($current !== null && $current->localName === $subject && $this->afeIndexOf($current) < 0) {
            $this->popOpen();
            return true;
        }
        for ($outer = 0; $outer < 8; $outer++) {
            $formatting = null;
            for ($i = count($this->afe) - 1; $i >= 0; $i--) {
                $entry = $this->afe[$i];
                if ($entry === null) {
                    break;
                }
                if ($entry->localName === $subject) {
                    $formatting = $entry;
                    break;
                }
            }
            if ($formatting === null) {
                return false;
            }
            $fIndex = $this->openIndex($formatting);
            if ($fIndex < 0) {
                array_splice($this->afe, $this->afeIndexOf($formatting), 1);
                return true;
            }
            if (!$this->inScope($formatting->localName)) {
                return true;
            }
            $furthest = null;
            $furthestIndex = -1;
            for ($i = $fIndex + 1; $i < count($this->open); $i++) {
                if ($this->isSpecialEl($this->open[$i])) {
                    $furthest = $this->open[$i];
                    $furthestIndex = $i;
                    break;
                }
            }
            if ($furthest === null) {
                while (count($this->open) > $fIndex) {
                    array_pop($this->open);
                }
                array_splice($this->afe, $this->afeIndexOf($formatting), 1);
                return true;
            }
            $commonAncestor = $this->open[$fIndex - 1];
            $bookmark = $this->afeIndexOf($formatting);
            $node = $furthest;
            $lastNode = $furthest;
            $nodeIndex = $furthestIndex;
            $inner = 0;
            while (true) {
                $inner++;
                $nodeIndex--;
                $node = $this->open[$nodeIndex];
                if ($node === $formatting) {
                    break;
                }
                $afeNode = $this->afeIndexOf($node);
                if ($inner > 3 && $afeNode >= 0) {
                    array_splice($this->afe, $afeNode, 1);
                    if ($afeNode < $bookmark) {
                        $bookmark--;
                    }
                    $afeNode = -1;
                }
                if ($afeNode < 0) {
                    array_splice($this->open, $nodeIndex, 1);
                    continue;
                }
                $attrs = [];
                foreach ($node->attributes->items as $attr) {
                    $attrs[] = [$attr->name, $attr->value];
                }
                $replacement = $this->makeElement($node->localName, $attrs);
                array_splice($this->afe, $afeNode, 1, [$replacement]);
                array_splice($this->open, $nodeIndex, 1, [$replacement]);
                $node = $replacement;
                if ($lastNode === $furthest) {
                    $bookmark = $afeNode + 1;
                }
                if ($lastNode->parentNode !== null) {
                    $lastNode->parentNode->removeChild($lastNode);
                }
                $node->insertBefore($lastNode, null);
                $lastNode = $node;
            }
            if ($lastNode->parentNode !== null) {
                $lastNode->parentNode->removeChild($lastNode);
            }
            $this->flushText();
            $before = null;
            $parent = $this->insertionParent($commonAncestor, $before);
            $parent->insertBefore($lastNode, $before);
            $attrs = [];
            foreach ($formatting->attributes->items as $attr) {
                $attrs[] = [$attr->name, $attr->value];
            }
            $new = $this->makeElement($formatting->localName, $attrs);
            while (count($furthest->kids) > 0) {
                $child = $furthest->kids[0];
                $furthest->removeChild($child);
                $new->insertBefore($child, null);
            }
            $furthest->insertBefore($new, null);
            $old = $this->afeIndexOf($formatting);
            array_splice($this->afe, $old, 1);
            if ($old < $bookmark) {
                $bookmark--;
            }
            array_splice($this->afe, $bookmark, 0, [$new]);
            $this->removeOpen($formatting);
            $at = $this->openIndex($furthest);
            array_splice($this->open, $at + 1, 0, [$new]);
        }
        return true;
    }

    // ---- insertion modes --------------------------------------------------------------------

    private function resetMode(): void
    {
        for ($i = count($this->open) - 1; $i >= 0; $i--) {
            $n = $this->open[$i]->localName;
            $last = $i === 0;
            if ($n === 'select') {
                $this->mode = 'in select';
                return;
            }
            if (($n === 'td' || $n === 'th') && !$last) {
                $this->mode = 'in cell';
                return;
            }
            if ($n === 'tr') {
                $this->mode = 'in row';
                return;
            }
            if ($n === 'tbody' || $n === 'thead' || $n === 'tfoot') {
                $this->mode = 'in table body';
                return;
            }
            if ($n === 'caption') {
                $this->mode = 'in caption';
                return;
            }
            if ($n === 'colgroup') {
                $this->mode = 'in column group';
                return;
            }
            if ($n === 'table') {
                $this->mode = 'in table';
                return;
            }
            if ($n === 'template') {
                $this->mode = count($this->templateModes) > 0 ? $this->templateModes[count($this->templateModes) - 1] : 'in template';
                return;
            }
            if ($n === 'head' && !$last) {
                $this->mode = 'in head';
                return;
            }
            if ($n === 'body') {
                $this->mode = 'in body';
                return;
            }
            if ($n === 'frameset') {
                $this->mode = 'in frameset';
                return;
            }
            if ($n === 'html') {
                $this->mode = $this->headEl === null ? 'before head' : 'after head';
                return;
            }
            if ($last) {
                $this->mode = 'in body';
                return;
            }
        }
        $this->mode = 'in body';
    }

    private function tbDoctype(string $name, ?string $public, ?string $system, bool $forceQuirks): void
    {
        if ($this->mode !== 'initial') {
            return;
        }
        $doctype = new \Dom__DocumentType($name, $public ?? '', $system ?? '');
        $doctype->ownerDocument = $this->doc;
        $this->doc->insertBefore($doctype, null);
        if ($name === 'html' && !$forceQuirks && $public === null && $system === null) {
            $this->quirks = false;
        } elseif ($name === 'html' && !$forceQuirks) {
            $this->quirks = false;
        } else {
            $this->quirks = true;
        }
        $this->mode = 'before html';
    }

    private function tbComment(string $text): void
    {
        $comment = new \Dom__Comment($text);
        $comment->ownerDocument = $this->doc;
        if ($this->mode === 'initial' || $this->mode === 'before html' || $this->mode === 'after after body') {
            $this->doc->insertBefore($comment, null);
            return;
        }
        if ($this->mode === 'after body') {
            $html = $this->open[0];
            $html->insertBefore($comment, null);
            return;
        }
        $this->insertNode($comment);
    }

    private function tbChars(string $text): void
    {
        // Whitespace handling per insertion mode: the leading whitespace of the text is dispatched
        // separately from the rest where the mode ignores it.
        $remaining = $text;
        if (($this->mode === 'in table' || $this->mode === 'in table body' || $this->mode === 'in row')
            && in_array($this->currName(), ['table', 'tbody', 'tfoot', 'thead', 'tr'], true)
            && !$this->inForeign('chars', '')) {
            $this->tableTextBuf .= $text;
            return;
        }
        if ($this->inForeign('chars', '')) {
            $this->insertCharsNow(str_replace("\0", "\u{fffd}", $remaining));
            if (trim($remaining, " \t\n\f") !== '') {
                $this->framesetOk = false;
            }
            return;
        }
        while ($remaining !== '') {
            $n = strlen($remaining);
            $i = 0;
            while ($i < $n && self::isSpace($remaining[$i])) {
                $i++;
            }
            if ($i > 0) {
                $this->tbWhitespace(substr($remaining, 0, $i));
                $remaining = substr($remaining, $i);
                continue;
            }
            $j = 0;
            while ($j < $n && !self::isSpace($remaining[$j])) {
                $j++;
            }
            $this->tbText(substr($remaining, 0, $j));
            $remaining = substr($remaining, $j);
        }
    }

    private function tbWhitespace(string $ws): void
    {
        $m = $this->mode;
        if ($m === 'initial' || $m === 'before html' || $m === 'before head') {
            return;
        }
        if ($m === 'in head' || $m === 'in head noscript' || $m === 'in column group' || $m === 'after head' || $m === 'in frameset' || $m === 'after frameset') {
            if ($m === 'in head noscript') {
                $this->insertCharsNow($ws);
                return;
            }
            $this->insertCharsNow($ws);
            return;
        }
        if ($m === 'after body' || $m === 'after after body' || $m === 'after after frameset') {
            $this->processUsing('in body', function () use ($ws) {
                $this->reconstructFormatting();
                $this->insertCharsNow($ws);
            });
            return;
        }
        if ($m === 'in table' || $m === 'in table body' || $m === 'in row') {
            $c = $this->currName();
            if (in_array($c, ['table', 'tbody', 'tfoot', 'thead', 'tr'], true)) {
                $this->insertCharsNow($ws);
                return;
            }
            $this->bodyChars($ws);
            return;
        }
        if ($m === 'in select') {
            $this->insertCharsNow($ws);
            return;
        }
        if ($m === 'text') {
            $this->insertCharsNow($ws);
            return;
        }
        $this->bodyChars($ws);
    }

    private function bodyChars(string $text): void
    {
        $this->reconstructFormatting();
        $this->insertCharsNow($text);
        if (trim($text, " \t\n\f") !== '') {
            $this->framesetOk = false;
        }
    }

    
    private function processUsing(string $mode, \Closure $fn): void
    {
        $saved = $this->mode;
        $this->mode = $mode;
        $fn();
        if ($this->mode === $mode) {
            $this->mode = $saved;
        }
    }

    private function tbText(string $text): void
    {
        $this->flushText();
        $m = $this->mode;
        if ($m === 'initial') {
            $this->quirks = true;
            $this->mode = 'before html';
            $this->tbText($text);
            return;
        }
        if ($m === 'before html') {
            $this->createHtml([]);
            $this->mode = 'before head';
            $this->tbText($text);
            return;
        }
        if ($m === 'before head') {
            $this->headEl = $this->insertElement('head', []);
            $this->mode = 'in head';
            $this->tbText($text);
            return;
        }
        if ($m === 'in head') {
            $this->popOpen();
            $this->mode = 'after head';
            $this->tbText($text);
            return;
        }
        if ($m === 'in head noscript') {
            $this->popOpen();
            $this->mode = 'in head';
            $this->tbText($text);
            return;
        }
        if ($m === 'after head') {
            $this->insertElement('body', []);
            $this->mode = 'in body';
            $this->tbText($text);
            return;
        }
        if ($m === 'text') {
            $this->insertCharsNow($text);
            return;
        }
        if ($m === 'in table' || $m === 'in table body' || $m === 'in row') {
            $c = $this->currName();
            if (in_array($c, ['table', 'tbody', 'tfoot', 'thead', 'tr'], true)) {
                $saved = $this->foster;
                $this->foster = true;
                $this->flushText();
                $this->bodyChars($text);
                $this->flushText();
                $this->foster = $saved;
                return;
            }
            $this->bodyChars($text);
            return;
        }
        if ($m === 'in column group') {
            if ($this->currName() === 'colgroup') {
                $this->popOpen();
                $this->mode = 'in table';
                $this->tbText($text);
            }
            return;
        }
        if ($m === 'in select') {
            $this->insertCharsNow($text);
            return;
        }
        if ($m === 'after body' || $m === 'after after body') {
            $this->mode = 'in body';
            $this->tbText($text);
            return;
        }
        if ($m === 'in frameset' || $m === 'after frameset' || $m === 'after after frameset') {
            return;
        }
        $this->bodyChars($text);
    }

    private function createHtml(array $attrs): \Dom__Element
    {
        $el = $this->makeElement('html', $attrs);
        $this->doc->insertBefore($el, null);
        $this->pushOpen($el);
        return $el;
    }

    private function mergeAttributes(\Dom__Element $into, array $attrs): void
    {
        foreach ($attrs as $pair) {
            if (!$into->hasAttribute($pair[0])) {
                $into->setAttribute($pair[0], $pair[1]);
            }
        }
    }

    private function tbStart(string $name, array $attrs, bool $selfClosing): void
    {
        $this->flushText();
        if ($this->inForeign('start', $name)) {
            $this->foreignStart($name, $attrs, $selfClosing);
            return;
        }
        $this->dispatchStart($name, $attrs, $selfClosing);
    }

    private function dispatchStart(string $name, array $attrs, bool $selfClosing): void
    {
        $m = $this->mode;
        if ($m === 'initial') {
            $this->quirks = true;
            $this->mode = 'before html';
            $this->dispatchStart($name, $attrs, $selfClosing);
            return;
        }
        if ($m === 'before html') {
            if ($name === 'html') {
                $this->createHtml($attrs);
                $this->mode = 'before head';
                return;
            }
            $this->createHtml([]);
            $this->mode = 'before head';
            $this->dispatchStart($name, $attrs, $selfClosing);
            return;
        }
        if ($m === 'before head') {
            if ($name === 'html') {
                $this->startInBody($name, $attrs, $selfClosing);
                return;
            }
            if ($name === 'head') {
                $this->headEl = $this->insertElement('head', $attrs);
                $this->mode = 'in head';
                return;
            }
            $this->headEl = $this->insertElement('head', []);
            $this->mode = 'in head';
            $this->dispatchStart($name, $attrs, $selfClosing);
            return;
        }
        if ($m === 'in head') {
            $this->startInHead($name, $attrs, $selfClosing);
            return;
        }
        if ($m === 'in head noscript') {
            if ($name === 'html') {
                $this->startInBody($name, $attrs, $selfClosing);
                return;
            }
            if (in_array($name, ['basefont', 'bgsound', 'link', 'meta', 'noframes', 'style'], true)) {
                $this->startInHead($name, $attrs, $selfClosing);
                return;
            }
            if ($name === 'head' || $name === 'noscript') {
                return;
            }
            $this->popOpen();
            $this->mode = 'in head';
            $this->dispatchStart($name, $attrs, $selfClosing);
            return;
        }
        if ($m === 'after head') {
            if ($name === 'html') {
                $this->startInBody($name, $attrs, $selfClosing);
                return;
            }
            if ($name === 'body') {
                $this->insertElement('body', $attrs);
                $this->framesetOk = false;
                $this->mode = 'in body';
                return;
            }
            if ($name === 'frameset') {
                $this->insertElement('frameset', $attrs);
                $this->mode = 'in frameset';
                return;
            }
            if (in_array($name, ['base', 'basefont', 'bgsound', 'link', 'meta', 'noframes', 'script', 'style', 'template', 'title'], true)) {
                $head = $this->headEl;
                $this->pushOpen($head);
                $this->startInHead($name, $attrs, $selfClosing);
                $this->removeOpen($head);
                return;
            }
            if ($name === 'head') {
                return;
            }
            $this->insertElement('body', []);
            $this->mode = 'in body';
            $this->dispatchStart($name, $attrs, $selfClosing);
            return;
        }
        if ($m === 'text') {
            return;
        }
        if ($m === 'in table') {
            $this->startInTable($name, $attrs, $selfClosing);
            return;
        }
        if ($m === 'in caption') {
            if (in_array($name, ['caption', 'col', 'colgroup', 'tbody', 'td', 'tfoot', 'th', 'thead', 'tr'], true)) {
                if ($this->inScope('caption', 'table')) {
                    $this->generateImpliedEndTags();
                    $this->popUntil('caption');
                    $this->clearFormattingToMarker();
                    $this->mode = 'in table';
                    $this->dispatchStart($name, $attrs, $selfClosing);
                }
                return;
            }
            $this->startInBody($name, $attrs, $selfClosing);
            return;
        }
        if ($m === 'in column group') {
            if ($name === 'html') {
                $this->startInBody($name, $attrs, $selfClosing);
                return;
            }
            if ($name === 'col') {
                $this->insertVoid('col', $attrs);
                return;
            }
            if ($name === 'template') {
                $this->startInHead($name, $attrs, $selfClosing);
                return;
            }
            if ($this->currName() !== 'colgroup') {
                return;
            }
            $this->popOpen();
            $this->mode = 'in table';
            $this->dispatchStart($name, $attrs, $selfClosing);
            return;
        }
        if ($m === 'in table body') {
            if ($name === 'tr') {
                $this->clearToContext(['tbody', 'tfoot', 'thead', 'template', 'html']);
                $this->insertElement('tr', $attrs);
                $this->mode = 'in row';
                return;
            }
            if ($name === 'th' || $name === 'td') {
                $this->clearToContext(['tbody', 'tfoot', 'thead', 'template', 'html']);
                $this->insertElement('tr', []);
                $this->mode = 'in row';
                $this->dispatchStart($name, $attrs, $selfClosing);
                return;
            }
            if (in_array($name, ['caption', 'col', 'colgroup', 'tbody', 'tfoot', 'thead'], true)) {
                if (!$this->tableBodyInScope()) {
                    return;
                }
                $this->clearToContext(['tbody', 'tfoot', 'thead', 'template', 'html']);
                $this->popOpen();
                $this->mode = 'in table';
                $this->dispatchStart($name, $attrs, $selfClosing);
                return;
            }
            $this->startInTable($name, $attrs, $selfClosing);
            return;
        }
        if ($m === 'in row') {
            if ($name === 'th' || $name === 'td') {
                $this->clearToContext(['tr', 'template', 'html']);
                $this->insertElement($name, $attrs);
                $this->mode = 'in cell';
                $this->afe[] = null;
                return;
            }
            if (in_array($name, ['caption', 'col', 'colgroup', 'tbody', 'tfoot', 'thead', 'tr'], true)) {
                if (!$this->inScope('tr', 'table')) {
                    return;
                }
                $this->clearToContext(['tr', 'template', 'html']);
                $this->popOpen();
                $this->mode = 'in table body';
                $this->dispatchStart($name, $attrs, $selfClosing);
                return;
            }
            $this->startInTable($name, $attrs, $selfClosing);
            return;
        }
        if ($m === 'in cell') {
            if (in_array($name, ['caption', 'col', 'colgroup', 'tbody', 'td', 'tfoot', 'th', 'thead', 'tr'], true)) {
                if (!$this->inScope('td', 'table') && !$this->inScope('th', 'table')) {
                    return;
                }
                $this->closeCell();
                $this->dispatchStart($name, $attrs, $selfClosing);
                return;
            }
            $this->startInBody($name, $attrs, $selfClosing);
            return;
        }
        if ($m === 'in select') {
            $this->startInSelect($name, $attrs, $selfClosing);
            return;
        }
        if ($m === 'after body') {
            if ($name === 'html') {
                $this->startInBody($name, $attrs, $selfClosing);
                return;
            }
            $this->mode = 'in body';
            $this->dispatchStart($name, $attrs, $selfClosing);
            return;
        }
        if ($m === 'after after body') {
            if ($name === 'html') {
                $this->startInBody($name, $attrs, $selfClosing);
                return;
            }
            $this->mode = 'in body';
            $this->dispatchStart($name, $attrs, $selfClosing);
            return;
        }
        if ($m === 'in frameset') {
            if ($name === 'html') {
                $this->startInBody($name, $attrs, $selfClosing);
            } elseif ($name === 'frameset') {
                $this->insertElement('frameset', $attrs);
            } elseif ($name === 'frame') {
                $this->insertVoid('frame', $attrs);
            } elseif ($name === 'noframes') {
                $this->startInHead($name, $attrs, $selfClosing);
            }
            return;
        }
        if ($m === 'after frameset' || $m === 'after after frameset') {
            if ($name === 'html') {
                $this->startInBody($name, $attrs, $selfClosing);
            } elseif ($name === 'noframes') {
                $this->startInHead($name, $attrs, $selfClosing);
            }
            return;
        }
        if ($m === 'in template') {
            $this->startInBody($name, $attrs, $selfClosing);
            return;
        }
        $this->startInBody($name, $attrs, $selfClosing);
    }

    private function clearToContext(array $names): void
    {
        while (count($this->open) > 0 && !in_array($this->currName(), $names, true)) {
            $this->popOpen();
        }
    }

    private function tableBodyInScope(): bool
    {
        return $this->inScope('tbody', 'table') || $this->inScope('thead', 'table') || $this->inScope('tfoot', 'table');
    }

    private function closeCell(): void
    {
        $this->generateImpliedEndTags();
        $this->popUntilOneOf(['td', 'th']);
        $this->clearFormattingToMarker();
        $this->mode = 'in row';
    }

    private function startInHead(string $name, array $attrs, bool $selfClosing): void
    {
        if ($name === 'html') {
            $this->startInBody($name, $attrs, $selfClosing);
            return;
        }
        if (in_array($name, ['base', 'basefont', 'bgsound', 'link', 'meta'], true)) {
            $this->insertVoid($name, $attrs);
            return;
        }
        if ($name === 'title') {
            $this->insertElement('title', $attrs);
            $this->startRaw('title', 'rcdata');
            return;
        }
        if ($name === 'noscript' && $this->mode === 'in head') {
            $this->insertElement('noscript', $attrs);
            $this->mode = 'in head noscript';
            return;
        }
        if ($name === 'noscript' || $name === 'noframes' || $name === 'style') {
            $this->insertElement($name, $attrs);
            $this->startRaw($name, 'rawtext');
            return;
        }
        if ($name === 'script') {
            $this->insertElement('script', $attrs);
            $this->startRaw('script', 'script');
            return;
        }
        if ($name === 'template') {
            $this->insertElement('template', $attrs);
            $this->afe[] = null;
            $this->framesetOk = false;
            $this->mode = 'in template';
            $this->templateModes[] = 'in template';
            return;
        }
        if ($name === 'head') {
            return;
        }
        $this->popOpen();
        $this->mode = 'after head';
        $this->dispatchStart($name, $attrs, $selfClosing);
    }

    private function startRaw(string $tag, string $mode): void
    {
        $this->rawTag = $tag;
        $this->rawMode = $mode;
        $this->origMode = $this->mode;
        $this->mode = 'text';
    }

    private function startInTable(string $name, array $attrs, bool $selfClosing): void
    {
        if ($name === 'caption') {
            $this->clearToContext(['table', 'template', 'html']);
            $this->afe[] = null;
            $this->insertElement('caption', $attrs);
            $this->mode = 'in caption';
            return;
        }
        if ($name === 'colgroup') {
            $this->clearToContext(['table', 'template', 'html']);
            $this->insertElement('colgroup', $attrs);
            $this->mode = 'in column group';
            return;
        }
        if ($name === 'col') {
            $this->clearToContext(['table', 'template', 'html']);
            $this->insertElement('colgroup', []);
            $this->mode = 'in column group';
            $this->dispatchStart($name, $attrs, $selfClosing);
            return;
        }
        if ($name === 'tbody' || $name === 'tfoot' || $name === 'thead') {
            $this->clearToContext(['table', 'template', 'html']);
            $this->insertElement($name, $attrs);
            $this->mode = 'in table body';
            return;
        }
        if ($name === 'td' || $name === 'th' || $name === 'tr') {
            $this->clearToContext(['table', 'template', 'html']);
            $this->insertElement('tbody', []);
            $this->mode = 'in table body';
            $this->dispatchStart($name, $attrs, $selfClosing);
            return;
        }
        if ($name === 'table') {
            if (!$this->inScope('table', 'table')) {
                return;
            }
            $this->popUntil('table');
            $this->resetMode();
            $this->dispatchStart($name, $attrs, $selfClosing);
            return;
        }
        if ($name === 'style' || $name === 'script' || $name === 'template') {
            $this->startInHead($name, $attrs, $selfClosing);
            return;
        }
        if ($name === 'input') {
            $type = '';
            foreach ($attrs as $pair) {
                if ($pair[0] === 'type') {
                    $type = strtolower($pair[1]);
                }
            }
            if ($type === 'hidden') {
                $this->insertVoid('input', $attrs);
                return;
            }
        }
        if ($name === 'form') {
            if ($this->formEl !== null || $this->stackHas('template')) {
                return;
            }
            $this->formEl = $this->insertVoid('form', $attrs);
            return;
        }
        $saved = $this->foster;
        $this->foster = true;
        $this->startInBody($name, $attrs, $selfClosing);
        $this->foster = $saved;
    }

    private function startInSelect(string $name, array $attrs, bool $selfClosing): void
    {
        if ($name === 'html') {
            $this->startInBody($name, $attrs, $selfClosing);
            return;
        }
        if ($name === 'option') {
            if ($this->currName() === 'option') {
                $this->popOpen();
            }
            $this->insertElement('option', $attrs);
            return;
        }
        if ($name === 'optgroup') {
            if ($this->currName() === 'option') {
                $this->popOpen();
            }
            if ($this->currName() === 'optgroup') {
                $this->popOpen();
            }
            $this->insertElement('optgroup', $attrs);
            return;
        }
        if ($name === 'select') {
            if (!$this->inScope('select', 'select')) {
                return;
            }
            $this->popUntil('select');
            $this->resetMode();
            return;
        }
        if ($name === 'input' || $name === 'keygen' || $name === 'textarea') {
            if (!$this->inScope('select', 'select')) {
                return;
            }
            $this->popUntil('select');
            $this->resetMode();
            $this->dispatchStart($name, $attrs, $selfClosing);
            return;
        }
        if ($name === 'script' || $name === 'template') {
            $this->startInHead($name, $attrs, $selfClosing);
            return;
        }
        // Any other start tag is inserted as it is (the customizable-select parsing the bundled
        // lexbor implements): no implied end tags, no formatting bookkeeping.
        if (in_array($name, ['area', 'br', 'embed', 'hr', 'img', 'keygen', 'wbr', 'param', 'source', 'track', 'col', 'base', 'basefont', 'bgsound', 'link', 'meta', 'frame'], true)) {
            $this->insertVoid($name, $attrs);
            return;
        }
        $this->insertElement($name, $attrs);
    }

    private function startInBody(string $name, array $attrs, bool $selfClosing): void
    {
        switch ($name) {
            case 'html':
                if ($this->stackHas('template')) {
                    return;
                }
                if (count($this->open) > 0) {
                    $this->mergeAttributes($this->open[0], $attrs);
                }
                return;
            case 'base':
            case 'basefont':
            case 'bgsound':
            case 'link':
            case 'meta':
            case 'noframes':
            case 'script':
            case 'style':
            case 'template':
            case 'title':
                $this->startInHead($name, $attrs, $selfClosing);
                return;
            case 'body':
                if (count($this->open) < 2 || $this->open[1]->localName !== 'body' || $this->stackHas('template')) {
                    return;
                }
                $this->framesetOk = false;
                $this->mergeAttributes($this->open[1], $attrs);
                return;
            case 'frameset':
                if (count($this->open) < 2 || $this->open[1]->localName !== 'body' || !$this->framesetOk) {
                    return;
                }
                $body = $this->open[1];
                if ($body->parentNode !== null) {
                    $body->parentNode->removeChild($body);
                }
                while (count($this->open) > 1) {
                    array_pop($this->open);
                }
                $this->insertElement('frameset', $attrs);
                $this->mode = 'in frameset';
                return;
            case 'address':
            case 'article':
            case 'aside':
            case 'blockquote':
            case 'center':
            case 'details':
            case 'dialog':
            case 'dir':
            case 'div':
            case 'dl':
            case 'fieldset':
            case 'figcaption':
            case 'figure':
            case 'footer':
            case 'header':
            case 'hgroup':
            case 'main':
            case 'menu':
            case 'nav':
            case 'ol':
            case 'p':
            case 'search':
            case 'section':
            case 'summary':
            case 'ul':
                $this->closePIfInButtonScope();
                $this->insertElement($name, $attrs);
                return;
            case 'h1':
            case 'h2':
            case 'h3':
            case 'h4':
            case 'h5':
            case 'h6':
                $this->closePIfInButtonScope();
                $c = $this->currName();
                if (strlen($c) === 2 && $c[0] === 'h' && $c[1] >= '1' && $c[1] <= '6') {
                    $this->popOpen();
                }
                $this->insertElement($name, $attrs);
                return;
            case 'pre':
            case 'listing':
                $this->closePIfInButtonScope();
                $this->insertElement($name, $attrs);
                $this->framesetOk = false;
                $this->skipNewline();
                return;
            case 'form':
                if ($this->formEl !== null && !$this->stackHas('template')) {
                    return;
                }
                $this->closePIfInButtonScope();
                $el = $this->insertElement('form', $attrs);
                if (!$this->stackHas('template')) {
                    $this->formEl = $el;
                }
                return;
            case 'li':
                $this->framesetOk = false;
                for ($i = count($this->open) - 1; $i >= 0; $i--) {
                    $n = $this->open[$i]->localName;
                    if ($n === 'li') {
                        $this->generateImpliedEndTags('li');
                        $this->popUntil('li');
                        break;
                    }
                    if ($this->isSpecialEl($this->open[$i]) && $n !== 'address' && $n !== 'div' && $n !== 'p') {
                        break;
                    }
                }
                $this->closePIfInButtonScope();
                $this->insertElement('li', $attrs);
                return;
            case 'dd':
            case 'dt':
                $this->framesetOk = false;
                for ($i = count($this->open) - 1; $i >= 0; $i--) {
                    $n = $this->open[$i]->localName;
                    if ($n === 'dd' || $n === 'dt') {
                        $this->generateImpliedEndTags($n);
                        $this->popUntil($n);
                        break;
                    }
                    if ($this->isSpecialEl($this->open[$i]) && $n !== 'address' && $n !== 'div' && $n !== 'p') {
                        break;
                    }
                }
                $this->closePIfInButtonScope();
                $this->insertElement($name, $attrs);
                return;
            case 'plaintext':
                $this->closePIfInButtonScope();
                $this->insertElement('plaintext', $attrs);
                $this->rawTag = 'plaintext';
                $this->rawMode = 'plaintext';
                return;
            case 'button':
                if ($this->inScope('button')) {
                    $this->generateImpliedEndTags();
                    $this->popUntil('button');
                }
                $this->reconstructFormatting();
                $this->insertElement('button', $attrs);
                $this->framesetOk = false;
                return;
            case 'a':
                for ($i = count($this->afe) - 1; $i >= 0; $i--) {
                    $entry = $this->afe[$i];
                    if ($entry === null) {
                        break;
                    }
                    if ($entry->localName === 'a') {
                        $this->adoptionAgency('a');
                        $at = $this->afeIndexOf($entry);
                        if ($at >= 0) {
                            array_splice($this->afe, $at, 1);
                        }
                        $this->removeOpen($entry);
                        break;
                    }
                }
                $this->reconstructFormatting();
                $el = $this->insertElement('a', $attrs);
                $this->afePush($el, $attrs);
                return;
            case 'b':
            case 'big':
            case 'code':
            case 'em':
            case 'font':
            case 'i':
            case 's':
            case 'small':
            case 'strike':
            case 'strong':
            case 'tt':
            case 'u':
                $this->reconstructFormatting();
                $el = $this->insertElement($name, $attrs);
                $this->afePush($el, $attrs);
                return;
            case 'nobr':
                $this->reconstructFormatting();
                if ($this->inScope('nobr')) {
                    $this->adoptionAgency('nobr');
                    $this->reconstructFormatting();
                }
                $el = $this->insertElement('nobr', $attrs);
                $this->afePush($el, $attrs);
                return;
            case 'applet':
            case 'marquee':
            case 'object':
                $this->reconstructFormatting();
                $this->insertElement($name, $attrs);
                $this->afe[] = null;
                $this->framesetOk = false;
                return;
            case 'table':
                if (!$this->quirks) {
                    $this->closePIfInButtonScope();
                }
                $this->insertElement('table', $attrs);
                $this->framesetOk = false;
                $this->mode = 'in table';
                return;
            case 'area':
            case 'br':
            case 'embed':
            case 'img':
            case 'keygen':
            case 'wbr':
                $this->reconstructFormatting();
                $this->insertVoid($name, $attrs);
                $this->framesetOk = false;
                return;
            case 'input':
                $this->reconstructFormatting();
                $this->insertVoid('input', $attrs);
                $type = '';
                foreach ($attrs as $pair) {
                    if ($pair[0] === 'type') {
                        $type = strtolower($pair[1]);
                    }
                }
                if ($type !== 'hidden') {
                    $this->framesetOk = false;
                }
                return;
            case 'param':
            case 'source':
            case 'track':
                $this->insertVoid($name, $attrs);
                return;
            case 'hr':
                $this->closePIfInButtonScope();
                $this->insertVoid('hr', $attrs);
                $this->framesetOk = false;
                return;
            case 'image':
                $this->startInBody('img', $attrs, $selfClosing);
                return;
            case 'textarea':
                $this->insertElement('textarea', $attrs);
                $this->skipNewline();
                $this->rawTag = 'textarea';
                $this->rawMode = 'rcdata';
                $this->origMode = $this->mode;
                $this->framesetOk = false;
                $this->mode = 'text';
                return;
            case 'xmp':
                $this->closePIfInButtonScope();
                $this->reconstructFormatting();
                $this->framesetOk = false;
                $this->insertElement('xmp', $attrs);
                $this->startRaw('xmp', 'rawtext');
                return;
            case 'iframe':
                $this->framesetOk = false;
                $this->insertElement('iframe', $attrs);
                $this->startRaw('iframe', 'rawtext');
                return;
            case 'noembed':
                $this->insertElement('noembed', $attrs);
                $this->startRaw('noembed', 'rawtext');
                return;
            case 'noscript':
                $this->reconstructFormatting();
                $this->insertElement('noscript', $attrs);
                return;
            case 'svg':
                $this->reconstructFormatting();
                $this->insertForeign('svg', $attrs, self::SVG);
                if ($selfClosing) {
                    $this->popOpen();
                }
                return;
            case 'math':
                $this->reconstructFormatting();
                $this->insertForeign('math', $attrs, self::MATHML);
                if ($selfClosing) {
                    $this->popOpen();
                }
                return;
            case 'select':
                $this->reconstructFormatting();
                $this->insertElement('select', $attrs);
                $this->framesetOk = false;
                if ($this->mode === 'in table' || $this->mode === 'in caption' || $this->mode === 'in table body' || $this->mode === 'in row' || $this->mode === 'in cell') {
                    $this->mode = 'in select';
                } else {
                    $this->mode = 'in select';
                }
                return;
            case 'optgroup':
            case 'option':
                if ($this->currName() === 'option') {
                    $this->popOpen();
                }
                $this->reconstructFormatting();
                $this->insertElement($name, $attrs);
                return;
            case 'rb':
            case 'rtc':
                if ($this->inScope('ruby')) {
                    $this->generateImpliedEndTags();
                }
                $this->insertElement($name, $attrs);
                return;
            case 'rp':
            case 'rt':
                if ($this->inScope('ruby')) {
                    $this->generateImpliedEndTags('rtc');
                }
                $this->insertElement($name, $attrs);
                return;
            case 'caption':
            case 'col':
            case 'colgroup':
            case 'frame':
            case 'head':
            case 'tbody':
            case 'td':
            case 'tfoot':
            case 'th':
            case 'thead':
            case 'tr':
                return;
        }
        $this->reconstructFormatting();
        $this->insertElement($name, $attrs);
        if ($selfClosing) {
            // acknowledged self-closing only matters for foreign content; HTML elements stay open
        }
    }

    private function skipNewline(): void
    {
        if ($this->peek() === "\n") {
            $this->pos++;
        }
    }

    private function tbEnd(string $name): void
    {
        $this->flushText();
        if ($this->inForeign('end', $name)) {
            $this->foreignEnd($name);
            return;
        }
        $this->dispatchEnd($name);
    }

    private function dispatchEnd(string $name): void
    {
        $m = $this->mode;
        if ($m === 'initial') {
            $this->quirks = true;
            $this->mode = 'before html';
            $this->dispatchEnd($name);
            return;
        }
        if ($m === 'before html') {
            if (in_array($name, ['head', 'body', 'html', 'br'], true)) {
                $this->createHtml([]);
                $this->mode = 'before head';
                $this->dispatchEnd($name);
            }
            return;
        }
        if ($m === 'before head') {
            if (in_array($name, ['head', 'body', 'html', 'br'], true)) {
                $this->headEl = $this->insertElement('head', []);
                $this->mode = 'in head';
                $this->dispatchEnd($name);
            }
            return;
        }
        if ($m === 'in head') {
            if ($name === 'head') {
                $this->popOpen();
                $this->mode = 'after head';
                return;
            }
            if ($name === 'body' || $name === 'html' || $name === 'br') {
                $this->popOpen();
                $this->mode = 'after head';
                $this->dispatchEnd($name);
                return;
            }
            if ($name === 'template') {
                $this->endTemplate();
            }
            return;
        }
        if ($m === 'in head noscript') {
            if ($name === 'noscript') {
                $this->popOpen();
                $this->mode = 'in head';
                return;
            }
            if ($name !== 'br') {
                return;
            }
            $this->popOpen();
            $this->mode = 'in head';
            $this->dispatchEnd($name);
            return;
        }
        if ($m === 'after head') {
            if ($name === 'template') {
                $this->endTemplate();
                return;
            }
            if ($name === 'body' || $name === 'html' || $name === 'br') {
                $this->insertElement('body', []);
                $this->mode = 'in body';
                $this->dispatchEnd($name);
            }
            return;
        }
        if ($m === 'text') {
            $this->popOpen();
            $this->mode = $this->origMode;
            return;
        }
        if ($m === 'in table') {
            if ($name === 'table') {
                if (!$this->inScope('table', 'table')) {
                    return;
                }
                $this->popUntil('table');
                $this->resetMode();
                return;
            }
            if (in_array($name, ['body', 'caption', 'col', 'colgroup', 'html', 'tbody', 'td', 'tfoot', 'th', 'thead', 'tr'], true)) {
                return;
            }
            if ($name === 'template') {
                $this->endTemplate();
                return;
            }
            $saved = $this->foster;
            $this->foster = true;
            $this->endInBody($name);
            $this->foster = $saved;
            return;
        }
        if ($m === 'in caption') {
            if ($name === 'caption') {
                if (!$this->inScope('caption', 'table')) {
                    return;
                }
                $this->generateImpliedEndTags();
                $this->popUntil('caption');
                $this->clearFormattingToMarker();
                $this->mode = 'in table';
                return;
            }
            if ($name === 'table') {
                if (!$this->inScope('caption', 'table')) {
                    return;
                }
                $this->generateImpliedEndTags();
                $this->popUntil('caption');
                $this->clearFormattingToMarker();
                $this->mode = 'in table';
                $this->dispatchEnd($name);
                return;
            }
            if (in_array($name, ['body', 'col', 'colgroup', 'html', 'tbody', 'td', 'tfoot', 'th', 'thead', 'tr'], true)) {
                return;
            }
            $this->endInBody($name);
            return;
        }
        if ($m === 'in column group') {
            if ($name === 'colgroup') {
                if ($this->currName() === 'colgroup') {
                    $this->popOpen();
                    $this->mode = 'in table';
                }
                return;
            }
            if ($name === 'col') {
                return;
            }
            if ($name === 'template') {
                $this->endTemplate();
                return;
            }
            if ($this->currName() === 'colgroup') {
                $this->popOpen();
                $this->mode = 'in table';
                $this->dispatchEnd($name);
            }
            return;
        }
        if ($m === 'in table body') {
            if ($name === 'tbody' || $name === 'tfoot' || $name === 'thead') {
                if (!$this->inScope($name, 'table')) {
                    return;
                }
                $this->clearToContext(['tbody', 'tfoot', 'thead', 'template', 'html']);
                $this->popOpen();
                $this->mode = 'in table';
                return;
            }
            if ($name === 'table') {
                if (!$this->tableBodyInScope()) {
                    return;
                }
                $this->clearToContext(['tbody', 'tfoot', 'thead', 'template', 'html']);
                $this->popOpen();
                $this->mode = 'in table';
                $this->dispatchEnd($name);
                return;
            }
            if (in_array($name, ['body', 'caption', 'col', 'colgroup', 'html', 'td', 'th', 'tr'], true)) {
                return;
            }
            $this->mode = 'in table';
            $this->dispatchEnd($name);
            $this->mode = $this->mode === 'in table' ? 'in table body' : $this->mode;
            return;
        }
        if ($m === 'in row') {
            if ($name === 'tr') {
                if (!$this->inScope('tr', 'table')) {
                    return;
                }
                $this->clearToContext(['tr', 'template', 'html']);
                $this->popOpen();
                $this->mode = 'in table body';
                return;
            }
            if ($name === 'table') {
                if (!$this->inScope('tr', 'table')) {
                    return;
                }
                $this->clearToContext(['tr', 'template', 'html']);
                $this->popOpen();
                $this->mode = 'in table body';
                $this->dispatchEnd($name);
                return;
            }
            if ($name === 'tbody' || $name === 'tfoot' || $name === 'thead') {
                if (!$this->inScope($name, 'table') || !$this->inScope('tr', 'table')) {
                    return;
                }
                $this->clearToContext(['tr', 'template', 'html']);
                $this->popOpen();
                $this->mode = 'in table body';
                $this->dispatchEnd($name);
                return;
            }
            if (in_array($name, ['body', 'caption', 'col', 'colgroup', 'html', 'td', 'th'], true)) {
                return;
            }
            $saved = $this->foster;
            $this->foster = true;
            $this->endInBody($name);
            $this->foster = $saved;
            return;
        }
        if ($m === 'in cell') {
            if ($name === 'td' || $name === 'th') {
                if (!$this->inScope($name, 'table')) {
                    return;
                }
                $this->generateImpliedEndTags();
                $this->popUntil($name);
                $this->clearFormattingToMarker();
                $this->mode = 'in row';
                return;
            }
            if (in_array($name, ['body', 'caption', 'col', 'colgroup', 'html'], true)) {
                return;
            }
            if (in_array($name, ['table', 'tbody', 'tfoot', 'thead', 'tr'], true)) {
                if (!$this->inScope($name, 'table')) {
                    return;
                }
                $this->closeCell();
                $this->dispatchEnd($name);
                return;
            }
            $this->endInBody($name);
            return;
        }
        if ($m === 'in select') {
            if ($name === 'optgroup') {
                if ($this->currName() === 'option' && count($this->open) > 1 && $this->open[count($this->open) - 2]->localName === 'optgroup') {
                    $this->popOpen();
                }
                if ($this->currName() === 'optgroup') {
                    $this->popOpen();
                }
                return;
            }
            if ($name === 'option') {
                if ($this->currName() === 'option') {
                    $this->popOpen();
                }
                return;
            }
            if ($name === 'select') {
                if (!$this->inScope('select', 'select')) {
                    return;
                }
                $this->popUntil('select');
                $this->resetMode();
                return;
            }
            if ($name === 'template') {
                $this->endTemplate();
                return;
            }
            $this->anyOtherEnd($name);
            return;
        }
        if ($m === 'after body') {
            if ($name === 'html') {
                $this->mode = 'after after body';
                return;
            }
            $this->mode = 'in body';
            $this->dispatchEnd($name);
            return;
        }
        if ($m === 'after after body') {
            $this->mode = 'in body';
            $this->dispatchEnd($name);
            return;
        }
        if ($m === 'in frameset') {
            if ($name === 'frameset' && $this->currName() !== 'html') {
                $this->popOpen();
                if ($this->currName() !== 'frameset') {
                    $this->mode = 'after frameset';
                }
            }
            return;
        }
        if ($m === 'after frameset') {
            if ($name === 'html') {
                $this->mode = 'after after frameset';
            }
            return;
        }
        if ($m === 'after after frameset') {
            return;
        }
        $this->endInBody($name);
    }

    private function endTemplate(): void
    {
        if (!$this->stackHas('template')) {
            return;
        }
        $this->generateAllImpliedEndTags();
        $this->popUntil('template');
        $this->clearFormattingToMarker();
        array_pop($this->templateModes);
        $this->resetMode();
    }

    private function endInBody(string $name): void
    {
        switch ($name) {
            case 'template':
                $this->endTemplate();
                return;
            case 'body':
                if (!$this->inScope('body')) {
                    return;
                }
                $this->mode = 'after body';
                return;
            case 'html':
                if (!$this->inScope('body')) {
                    return;
                }
                $this->mode = 'after body';
                $this->dispatchEnd('html');
                return;
            case 'address':
            case 'article':
            case 'aside':
            case 'blockquote':
            case 'button':
            case 'center':
            case 'details':
            case 'dialog':
            case 'dir':
            case 'div':
            case 'dl':
            case 'fieldset':
            case 'figcaption':
            case 'figure':
            case 'footer':
            case 'header':
            case 'hgroup':
            case 'listing':
            case 'main':
            case 'menu':
            case 'nav':
            case 'ol':
            case 'pre':
            case 'search':
            case 'section':
            case 'summary':
            case 'ul':
                if (!$this->inScope($name)) {
                    return;
                }
                $this->generateImpliedEndTags();
                $this->popUntil($name);
                return;
            case 'form':
                if (!$this->stackHas('template')) {
                    $node = $this->formEl;
                    $this->formEl = null;
                    if ($node === null || !$this->inScope('form') || $this->openIndex($node) < 0) {
                        return;
                    }
                    $this->generateImpliedEndTags();
                    $this->removeOpen($node);
                    return;
                }
                if (!$this->inScope('form')) {
                    return;
                }
                $this->generateImpliedEndTags();
                $this->popUntil('form');
                return;
            case 'p':
                if (!$this->inScope('p', 'button')) {
                    $this->insertElement('p', []);
                }
                $this->closeP();
                return;
            case 'li':
                if (!$this->inScope('li', 'list')) {
                    return;
                }
                $this->generateImpliedEndTags('li');
                $this->popUntil('li');
                return;
            case 'dd':
            case 'dt':
                if (!$this->inScope($name)) {
                    return;
                }
                $this->generateImpliedEndTags($name);
                $this->popUntil($name);
                return;
            case 'h1':
            case 'h2':
            case 'h3':
            case 'h4':
            case 'h5':
            case 'h6':
                if (!$this->headingInScope()) {
                    return;
                }
                $this->generateImpliedEndTags();
                $this->popUntilOneOf(['h1', 'h2', 'h3', 'h4', 'h5', 'h6']);
                return;
            case 'a':
            case 'b':
            case 'big':
            case 'code':
            case 'em':
            case 'font':
            case 'i':
            case 'nobr':
            case 's':
            case 'small':
            case 'strike':
            case 'strong':
            case 'tt':
            case 'u':
                if (!$this->adoptionAgency($name)) {
                    $this->anyOtherEnd($name);
                }
                return;
            case 'applet':
            case 'marquee':
            case 'object':
                if (!$this->inScope($name)) {
                    return;
                }
                $this->generateImpliedEndTags();
                $this->popUntil($name);
                $this->clearFormattingToMarker();
                return;
            case 'br':
                $this->reconstructFormatting();
                $this->insertVoid('br', []);
                $this->framesetOk = false;
                return;
        }
        $this->anyOtherEnd($name);
    }

    private function anyOtherEnd(string $name): void
    {
        for ($i = count($this->open) - 1; $i >= 0; $i--) {
            $node = $this->open[$i];
            if ($node->localName === $name) {
                $this->generateImpliedEndTags($name);
                while (count($this->open) > $i) {
                    array_pop($this->open);
                }
                return;
            }
            if ($this->isSpecialEl($node)) {
                return;
            }
        }
    }

    private function tbEof(): void
    {
        // Make sure the document has its html / head / body skeleton, as the spec's "stop parsing"
        // would after the implied tokens.
        $this->flushText();
        if ($this->mode === 'text') {
            $this->popOpen();
            $this->mode = $this->origMode;
        }
        if ($this->mode === 'in head noscript') {
            $this->popOpen();
            $this->mode = 'in head';
        }
        if ($this->mode === 'initial') {
            $this->quirks = true;
            $this->mode = 'before html';
        }
        if ($this->mode === 'before html') {
            $this->createHtml([]);
            $this->mode = 'before head';
        }
        if ($this->mode === 'before head') {
            $this->headEl = $this->insertElement('head', []);
            $this->mode = 'in head';
        }
        if ($this->mode === 'in head') {
            $this->popOpen();
            $this->mode = 'after head';
        }
        if ($this->mode === 'after head') {
            $this->insertElement('body', []);
            $this->mode = 'in body';
        }
        $this->flushText();
    }
}

function __elephc_dom_entity_table(): array
{
    return [
        'AElig;' => "\u{c6}",
        'AMP;' => "\u{26}",
        'Aacute;' => "\u{c1}",
        'Abreve;' => "\u{102}",
        'Acirc;' => "\u{c2}",
        'Acy;' => "\u{410}",
        'Afr;' => "\u{1d504}",
        'Agrave;' => "\u{c0}",
        'Alpha;' => "\u{391}",
        'Amacr;' => "\u{100}",
        'And;' => "\u{2a53}",
        'Aogon;' => "\u{104}",
        'Aopf;' => "\u{1d538}",
        'ApplyFunction;' => "\u{2061}",
        'Aring;' => "\u{c5}",
        'Ascr;' => "\u{1d49c}",
        'Assign;' => "\u{2254}",
        'Atilde;' => "\u{c3}",
        'Auml;' => "\u{c4}",
        'Backslash;' => "\u{2216}",
        'Barv;' => "\u{2ae7}",
        'Barwed;' => "\u{2306}",
        'Bcy;' => "\u{411}",
        'Because;' => "\u{2235}",
        'Bernoullis;' => "\u{212c}",
        'Beta;' => "\u{392}",
        'Bfr;' => "\u{1d505}",
        'Bopf;' => "\u{1d539}",
        'Breve;' => "\u{2d8}",
        'Bscr;' => "\u{212c}",
        'Bumpeq;' => "\u{224e}",
        'CHcy;' => "\u{427}",
        'COPY;' => "\u{a9}",
        'Cacute;' => "\u{106}",
        'Cap;' => "\u{22d2}",
        'CapitalDifferentialD;' => "\u{2145}",
        'Cayleys;' => "\u{212d}",
        'Ccaron;' => "\u{10c}",
        'Ccedil;' => "\u{c7}",
        'Ccirc;' => "\u{108}",
        'Cconint;' => "\u{2230}",
        'Cdot;' => "\u{10a}",
        'Cedilla;' => "\u{b8}",
        'CenterDot;' => "\u{b7}",
        'Cfr;' => "\u{212d}",
        'Chi;' => "\u{3a7}",
        'CircleDot;' => "\u{2299}",
        'CircleMinus;' => "\u{2296}",
        'CirclePlus;' => "\u{2295}",
        'CircleTimes;' => "\u{2297}",
        'ClockwiseContourIntegral;' => "\u{2232}",
        'CloseCurlyDoubleQuote;' => "\u{201d}",
        'CloseCurlyQuote;' => "\u{2019}",
        'Colon;' => "\u{2237}",
        'Colone;' => "\u{2a74}",
        'Congruent;' => "\u{2261}",
        'Conint;' => "\u{222f}",
        'ContourIntegral;' => "\u{222e}",
        'Copf;' => "\u{2102}",
        'Coproduct;' => "\u{2210}",
        'CounterClockwiseContourIntegral;' => "\u{2233}",
        'Cross;' => "\u{2a2f}",
        'Cscr;' => "\u{1d49e}",
        'Cup;' => "\u{22d3}",
        'CupCap;' => "\u{224d}",
        'DD;' => "\u{2145}",
        'DDotrahd;' => "\u{2911}",
        'DJcy;' => "\u{402}",
        'DScy;' => "\u{405}",
        'DZcy;' => "\u{40f}",
        'Dagger;' => "\u{2021}",
        'Darr;' => "\u{21a1}",
        'Dashv;' => "\u{2ae4}",
        'Dcaron;' => "\u{10e}",
        'Dcy;' => "\u{414}",
        'Del;' => "\u{2207}",
        'Delta;' => "\u{394}",
        'Dfr;' => "\u{1d507}",
        'DiacriticalAcute;' => "\u{b4}",
        'DiacriticalDot;' => "\u{2d9}",
        'DiacriticalDoubleAcute;' => "\u{2dd}",
        'DiacriticalGrave;' => "\u{60}",
        'DiacriticalTilde;' => "\u{2dc}",
        'Diamond;' => "\u{22c4}",
        'DifferentialD;' => "\u{2146}",
        'Dopf;' => "\u{1d53b}",
        'Dot;' => "\u{a8}",
        'DotDot;' => "\u{20dc}",
        'DotEqual;' => "\u{2250}",
        'DoubleContourIntegral;' => "\u{222f}",
        'DoubleDot;' => "\u{a8}",
        'DoubleDownArrow;' => "\u{21d3}",
        'DoubleLeftArrow;' => "\u{21d0}",
        'DoubleLeftRightArrow;' => "\u{21d4}",
        'DoubleLeftTee;' => "\u{2ae4}",
        'DoubleLongLeftArrow;' => "\u{27f8}",
        'DoubleLongLeftRightArrow;' => "\u{27fa}",
        'DoubleLongRightArrow;' => "\u{27f9}",
        'DoubleRightArrow;' => "\u{21d2}",
        'DoubleRightTee;' => "\u{22a8}",
        'DoubleUpArrow;' => "\u{21d1}",
        'DoubleUpDownArrow;' => "\u{21d5}",
        'DoubleVerticalBar;' => "\u{2225}",
        'DownArrow;' => "\u{2193}",
        'DownArrowBar;' => "\u{2913}",
        'DownArrowUpArrow;' => "\u{21f5}",
        'DownBreve;' => "\u{311}",
        'DownLeftRightVector;' => "\u{2950}",
        'DownLeftTeeVector;' => "\u{295e}",
        'DownLeftVector;' => "\u{21bd}",
        'DownLeftVectorBar;' => "\u{2956}",
        'DownRightTeeVector;' => "\u{295f}",
        'DownRightVector;' => "\u{21c1}",
        'DownRightVectorBar;' => "\u{2957}",
        'DownTee;' => "\u{22a4}",
        'DownTeeArrow;' => "\u{21a7}",
        'Downarrow;' => "\u{21d3}",
        'Dscr;' => "\u{1d49f}",
        'Dstrok;' => "\u{110}",
        'ENG;' => "\u{14a}",
        'ETH;' => "\u{d0}",
        'Eacute;' => "\u{c9}",
        'Ecaron;' => "\u{11a}",
        'Ecirc;' => "\u{ca}",
        'Ecy;' => "\u{42d}",
        'Edot;' => "\u{116}",
        'Efr;' => "\u{1d508}",
        'Egrave;' => "\u{c8}",
        'Element;' => "\u{2208}",
        'Emacr;' => "\u{112}",
        'EmptySmallSquare;' => "\u{25fb}",
        'EmptyVerySmallSquare;' => "\u{25ab}",
        'Eogon;' => "\u{118}",
        'Eopf;' => "\u{1d53c}",
        'Epsilon;' => "\u{395}",
        'Equal;' => "\u{2a75}",
        'EqualTilde;' => "\u{2242}",
        'Equilibrium;' => "\u{21cc}",
        'Escr;' => "\u{2130}",
        'Esim;' => "\u{2a73}",
        'Eta;' => "\u{397}",
        'Euml;' => "\u{cb}",
        'Exists;' => "\u{2203}",
        'ExponentialE;' => "\u{2147}",
        'Fcy;' => "\u{424}",
        'Ffr;' => "\u{1d509}",
        'FilledSmallSquare;' => "\u{25fc}",
        'FilledVerySmallSquare;' => "\u{25aa}",
        'Fopf;' => "\u{1d53d}",
        'ForAll;' => "\u{2200}",
        'Fouriertrf;' => "\u{2131}",
        'Fscr;' => "\u{2131}",
        'GJcy;' => "\u{403}",
        'GT;' => "\u{3e}",
        'Gamma;' => "\u{393}",
        'Gammad;' => "\u{3dc}",
        'Gbreve;' => "\u{11e}",
        'Gcedil;' => "\u{122}",
        'Gcirc;' => "\u{11c}",
        'Gcy;' => "\u{413}",
        'Gdot;' => "\u{120}",
        'Gfr;' => "\u{1d50a}",
        'Gg;' => "\u{22d9}",
        'Gopf;' => "\u{1d53e}",
        'GreaterEqual;' => "\u{2265}",
        'GreaterEqualLess;' => "\u{22db}",
        'GreaterFullEqual;' => "\u{2267}",
        'GreaterGreater;' => "\u{2aa2}",
        'GreaterLess;' => "\u{2277}",
        'GreaterSlantEqual;' => "\u{2a7e}",
        'GreaterTilde;' => "\u{2273}",
        'Gscr;' => "\u{1d4a2}",
        'Gt;' => "\u{226b}",
        'HARDcy;' => "\u{42a}",
        'Hacek;' => "\u{2c7}",
        'Hat;' => "\u{5e}",
        'Hcirc;' => "\u{124}",
        'Hfr;' => "\u{210c}",
        'HilbertSpace;' => "\u{210b}",
        'Hopf;' => "\u{210d}",
        'HorizontalLine;' => "\u{2500}",
        'Hscr;' => "\u{210b}",
        'Hstrok;' => "\u{126}",
        'HumpDownHump;' => "\u{224e}",
        'HumpEqual;' => "\u{224f}",
        'IEcy;' => "\u{415}",
        'IJlig;' => "\u{132}",
        'IOcy;' => "\u{401}",
        'Iacute;' => "\u{cd}",
        'Icirc;' => "\u{ce}",
        'Icy;' => "\u{418}",
        'Idot;' => "\u{130}",
        'Ifr;' => "\u{2111}",
        'Igrave;' => "\u{cc}",
        'Im;' => "\u{2111}",
        'Imacr;' => "\u{12a}",
        'ImaginaryI;' => "\u{2148}",
        'Implies;' => "\u{21d2}",
        'Int;' => "\u{222c}",
        'Integral;' => "\u{222b}",
        'Intersection;' => "\u{22c2}",
        'InvisibleComma;' => "\u{2063}",
        'InvisibleTimes;' => "\u{2062}",
        'Iogon;' => "\u{12e}",
        'Iopf;' => "\u{1d540}",
        'Iota;' => "\u{399}",
        'Iscr;' => "\u{2110}",
        'Itilde;' => "\u{128}",
        'Iukcy;' => "\u{406}",
        'Iuml;' => "\u{cf}",
        'Jcirc;' => "\u{134}",
        'Jcy;' => "\u{419}",
        'Jfr;' => "\u{1d50d}",
        'Jopf;' => "\u{1d541}",
        'Jscr;' => "\u{1d4a5}",
        'Jsercy;' => "\u{408}",
        'Jukcy;' => "\u{404}",
        'KHcy;' => "\u{425}",
        'KJcy;' => "\u{40c}",
        'Kappa;' => "\u{39a}",
        'Kcedil;' => "\u{136}",
        'Kcy;' => "\u{41a}",
        'Kfr;' => "\u{1d50e}",
        'Kopf;' => "\u{1d542}",
        'Kscr;' => "\u{1d4a6}",
        'LJcy;' => "\u{409}",
        'LT;' => "\u{3c}",
        'Lacute;' => "\u{139}",
        'Lambda;' => "\u{39b}",
        'Lang;' => "\u{27ea}",
        'Laplacetrf;' => "\u{2112}",
        'Larr;' => "\u{219e}",
        'Lcaron;' => "\u{13d}",
        'Lcedil;' => "\u{13b}",
        'Lcy;' => "\u{41b}",
        'LeftAngleBracket;' => "\u{27e8}",
        'LeftArrow;' => "\u{2190}",
        'LeftArrowBar;' => "\u{21e4}",
        'LeftArrowRightArrow;' => "\u{21c6}",
        'LeftCeiling;' => "\u{2308}",
        'LeftDoubleBracket;' => "\u{27e6}",
        'LeftDownTeeVector;' => "\u{2961}",
        'LeftDownVector;' => "\u{21c3}",
        'LeftDownVectorBar;' => "\u{2959}",
        'LeftFloor;' => "\u{230a}",
        'LeftRightArrow;' => "\u{2194}",
        'LeftRightVector;' => "\u{294e}",
        'LeftTee;' => "\u{22a3}",
        'LeftTeeArrow;' => "\u{21a4}",
        'LeftTeeVector;' => "\u{295a}",
        'LeftTriangle;' => "\u{22b2}",
        'LeftTriangleBar;' => "\u{29cf}",
        'LeftTriangleEqual;' => "\u{22b4}",
        'LeftUpDownVector;' => "\u{2951}",
        'LeftUpTeeVector;' => "\u{2960}",
        'LeftUpVector;' => "\u{21bf}",
        'LeftUpVectorBar;' => "\u{2958}",
        'LeftVector;' => "\u{21bc}",
        'LeftVectorBar;' => "\u{2952}",
        'Leftarrow;' => "\u{21d0}",
        'Leftrightarrow;' => "\u{21d4}",
        'LessEqualGreater;' => "\u{22da}",
        'LessFullEqual;' => "\u{2266}",
        'LessGreater;' => "\u{2276}",
        'LessLess;' => "\u{2aa1}",
        'LessSlantEqual;' => "\u{2a7d}",
        'LessTilde;' => "\u{2272}",
        'Lfr;' => "\u{1d50f}",
        'Ll;' => "\u{22d8}",
        'Lleftarrow;' => "\u{21da}",
        'Lmidot;' => "\u{13f}",
        'LongLeftArrow;' => "\u{27f5}",
        'LongLeftRightArrow;' => "\u{27f7}",
        'LongRightArrow;' => "\u{27f6}",
        'Longleftarrow;' => "\u{27f8}",
        'Longleftrightarrow;' => "\u{27fa}",
        'Longrightarrow;' => "\u{27f9}",
        'Lopf;' => "\u{1d543}",
        'LowerLeftArrow;' => "\u{2199}",
        'LowerRightArrow;' => "\u{2198}",
        'Lscr;' => "\u{2112}",
        'Lsh;' => "\u{21b0}",
        'Lstrok;' => "\u{141}",
        'Lt;' => "\u{226a}",
        'Map;' => "\u{2905}",
        'Mcy;' => "\u{41c}",
        'MediumSpace;' => "\u{205f}",
        'Mellintrf;' => "\u{2133}",
        'Mfr;' => "\u{1d510}",
        'MinusPlus;' => "\u{2213}",
        'Mopf;' => "\u{1d544}",
        'Mscr;' => "\u{2133}",
        'Mu;' => "\u{39c}",
        'NJcy;' => "\u{40a}",
        'Nacute;' => "\u{143}",
        'Ncaron;' => "\u{147}",
        'Ncedil;' => "\u{145}",
        'Ncy;' => "\u{41d}",
        'NegativeMediumSpace;' => "\u{200b}",
        'NegativeThickSpace;' => "\u{200b}",
        'NegativeThinSpace;' => "\u{200b}",
        'NegativeVeryThinSpace;' => "\u{200b}",
        'NestedGreaterGreater;' => "\u{226b}",
        'NestedLessLess;' => "\u{226a}",
        'NewLine;' => "\u{a}",
        'Nfr;' => "\u{1d511}",
        'NoBreak;' => "\u{2060}",
        'NonBreakingSpace;' => "\u{a0}",
        'Nopf;' => "\u{2115}",
        'Not;' => "\u{2aec}",
        'NotCongruent;' => "\u{2262}",
        'NotCupCap;' => "\u{226d}",
        'NotDoubleVerticalBar;' => "\u{2226}",
        'NotElement;' => "\u{2209}",
        'NotEqual;' => "\u{2260}",
        'NotEqualTilde;' => "\u{2242}\u{338}",
        'NotExists;' => "\u{2204}",
        'NotGreater;' => "\u{226f}",
        'NotGreaterEqual;' => "\u{2271}",
        'NotGreaterFullEqual;' => "\u{2267}\u{338}",
        'NotGreaterGreater;' => "\u{226b}\u{338}",
        'NotGreaterLess;' => "\u{2279}",
        'NotGreaterSlantEqual;' => "\u{2a7e}\u{338}",
        'NotGreaterTilde;' => "\u{2275}",
        'NotHumpDownHump;' => "\u{224e}\u{338}",
        'NotHumpEqual;' => "\u{224f}\u{338}",
        'NotLeftTriangle;' => "\u{22ea}",
        'NotLeftTriangleBar;' => "\u{29cf}\u{338}",
        'NotLeftTriangleEqual;' => "\u{22ec}",
        'NotLess;' => "\u{226e}",
        'NotLessEqual;' => "\u{2270}",
        'NotLessGreater;' => "\u{2278}",
        'NotLessLess;' => "\u{226a}\u{338}",
        'NotLessSlantEqual;' => "\u{2a7d}\u{338}",
        'NotLessTilde;' => "\u{2274}",
        'NotNestedGreaterGreater;' => "\u{2aa2}\u{338}",
        'NotNestedLessLess;' => "\u{2aa1}\u{338}",
        'NotPrecedes;' => "\u{2280}",
        'NotPrecedesEqual;' => "\u{2aaf}\u{338}",
        'NotPrecedesSlantEqual;' => "\u{22e0}",
        'NotReverseElement;' => "\u{220c}",
        'NotRightTriangle;' => "\u{22eb}",
        'NotRightTriangleBar;' => "\u{29d0}\u{338}",
        'NotRightTriangleEqual;' => "\u{22ed}",
        'NotSquareSubset;' => "\u{228f}\u{338}",
        'NotSquareSubsetEqual;' => "\u{22e2}",
        'NotSquareSuperset;' => "\u{2290}\u{338}",
        'NotSquareSupersetEqual;' => "\u{22e3}",
        'NotSubset;' => "\u{2282}\u{20d2}",
        'NotSubsetEqual;' => "\u{2288}",
        'NotSucceeds;' => "\u{2281}",
        'NotSucceedsEqual;' => "\u{2ab0}\u{338}",
        'NotSucceedsSlantEqual;' => "\u{22e1}",
        'NotSucceedsTilde;' => "\u{227f}\u{338}",
        'NotSuperset;' => "\u{2283}\u{20d2}",
        'NotSupersetEqual;' => "\u{2289}",
        'NotTilde;' => "\u{2241}",
        'NotTildeEqual;' => "\u{2244}",
        'NotTildeFullEqual;' => "\u{2247}",
        'NotTildeTilde;' => "\u{2249}",
        'NotVerticalBar;' => "\u{2224}",
        'Nscr;' => "\u{1d4a9}",
        'Ntilde;' => "\u{d1}",
        'Nu;' => "\u{39d}",
        'OElig;' => "\u{152}",
        'Oacute;' => "\u{d3}",
        'Ocirc;' => "\u{d4}",
        'Ocy;' => "\u{41e}",
        'Odblac;' => "\u{150}",
        'Ofr;' => "\u{1d512}",
        'Ograve;' => "\u{d2}",
        'Omacr;' => "\u{14c}",
        'Omega;' => "\u{3a9}",
        'Omicron;' => "\u{39f}",
        'Oopf;' => "\u{1d546}",
        'OpenCurlyDoubleQuote;' => "\u{201c}",
        'OpenCurlyQuote;' => "\u{2018}",
        'Or;' => "\u{2a54}",
        'Oscr;' => "\u{1d4aa}",
        'Oslash;' => "\u{d8}",
        'Otilde;' => "\u{d5}",
        'Otimes;' => "\u{2a37}",
        'Ouml;' => "\u{d6}",
        'OverBar;' => "\u{203e}",
        'OverBrace;' => "\u{23de}",
        'OverBracket;' => "\u{23b4}",
        'OverParenthesis;' => "\u{23dc}",
        'PartialD;' => "\u{2202}",
        'Pcy;' => "\u{41f}",
        'Pfr;' => "\u{1d513}",
        'Phi;' => "\u{3a6}",
        'Pi;' => "\u{3a0}",
        'PlusMinus;' => "\u{b1}",
        'Poincareplane;' => "\u{210c}",
        'Popf;' => "\u{2119}",
        'Pr;' => "\u{2abb}",
        'Precedes;' => "\u{227a}",
        'PrecedesEqual;' => "\u{2aaf}",
        'PrecedesSlantEqual;' => "\u{227c}",
        'PrecedesTilde;' => "\u{227e}",
        'Prime;' => "\u{2033}",
        'Product;' => "\u{220f}",
        'Proportion;' => "\u{2237}",
        'Proportional;' => "\u{221d}",
        'Pscr;' => "\u{1d4ab}",
        'Psi;' => "\u{3a8}",
        'QUOT;' => "\u{22}",
        'Qfr;' => "\u{1d514}",
        'Qopf;' => "\u{211a}",
        'Qscr;' => "\u{1d4ac}",
        'RBarr;' => "\u{2910}",
        'REG;' => "\u{ae}",
        'Racute;' => "\u{154}",
        'Rang;' => "\u{27eb}",
        'Rarr;' => "\u{21a0}",
        'Rarrtl;' => "\u{2916}",
        'Rcaron;' => "\u{158}",
        'Rcedil;' => "\u{156}",
        'Rcy;' => "\u{420}",
        'Re;' => "\u{211c}",
        'ReverseElement;' => "\u{220b}",
        'ReverseEquilibrium;' => "\u{21cb}",
        'ReverseUpEquilibrium;' => "\u{296f}",
        'Rfr;' => "\u{211c}",
        'Rho;' => "\u{3a1}",
        'RightAngleBracket;' => "\u{27e9}",
        'RightArrow;' => "\u{2192}",
        'RightArrowBar;' => "\u{21e5}",
        'RightArrowLeftArrow;' => "\u{21c4}",
        'RightCeiling;' => "\u{2309}",
        'RightDoubleBracket;' => "\u{27e7}",
        'RightDownTeeVector;' => "\u{295d}",
        'RightDownVector;' => "\u{21c2}",
        'RightDownVectorBar;' => "\u{2955}",
        'RightFloor;' => "\u{230b}",
        'RightTee;' => "\u{22a2}",
        'RightTeeArrow;' => "\u{21a6}",
        'RightTeeVector;' => "\u{295b}",
        'RightTriangle;' => "\u{22b3}",
        'RightTriangleBar;' => "\u{29d0}",
        'RightTriangleEqual;' => "\u{22b5}",
        'RightUpDownVector;' => "\u{294f}",
        'RightUpTeeVector;' => "\u{295c}",
        'RightUpVector;' => "\u{21be}",
        'RightUpVectorBar;' => "\u{2954}",
        'RightVector;' => "\u{21c0}",
        'RightVectorBar;' => "\u{2953}",
        'Rightarrow;' => "\u{21d2}",
        'Ropf;' => "\u{211d}",
        'RoundImplies;' => "\u{2970}",
        'Rrightarrow;' => "\u{21db}",
        'Rscr;' => "\u{211b}",
        'Rsh;' => "\u{21b1}",
        'RuleDelayed;' => "\u{29f4}",
        'SHCHcy;' => "\u{429}",
        'SHcy;' => "\u{428}",
        'SOFTcy;' => "\u{42c}",
        'Sacute;' => "\u{15a}",
        'Sc;' => "\u{2abc}",
        'Scaron;' => "\u{160}",
        'Scedil;' => "\u{15e}",
        'Scirc;' => "\u{15c}",
        'Scy;' => "\u{421}",
        'Sfr;' => "\u{1d516}",
        'ShortDownArrow;' => "\u{2193}",
        'ShortLeftArrow;' => "\u{2190}",
        'ShortRightArrow;' => "\u{2192}",
        'ShortUpArrow;' => "\u{2191}",
        'Sigma;' => "\u{3a3}",
        'SmallCircle;' => "\u{2218}",
        'Sopf;' => "\u{1d54a}",
        'Sqrt;' => "\u{221a}",
        'Square;' => "\u{25a1}",
        'SquareIntersection;' => "\u{2293}",
        'SquareSubset;' => "\u{228f}",
        'SquareSubsetEqual;' => "\u{2291}",
        'SquareSuperset;' => "\u{2290}",
        'SquareSupersetEqual;' => "\u{2292}",
        'SquareUnion;' => "\u{2294}",
        'Sscr;' => "\u{1d4ae}",
        'Star;' => "\u{22c6}",
        'Sub;' => "\u{22d0}",
        'Subset;' => "\u{22d0}",
        'SubsetEqual;' => "\u{2286}",
        'Succeeds;' => "\u{227b}",
        'SucceedsEqual;' => "\u{2ab0}",
        'SucceedsSlantEqual;' => "\u{227d}",
        'SucceedsTilde;' => "\u{227f}",
        'SuchThat;' => "\u{220b}",
        'Sum;' => "\u{2211}",
        'Sup;' => "\u{22d1}",
        'Superset;' => "\u{2283}",
        'SupersetEqual;' => "\u{2287}",
        'Supset;' => "\u{22d1}",
        'THORN;' => "\u{de}",
        'TRADE;' => "\u{2122}",
        'TSHcy;' => "\u{40b}",
        'TScy;' => "\u{426}",
        'Tab;' => "\u{9}",
        'Tau;' => "\u{3a4}",
        'Tcaron;' => "\u{164}",
        'Tcedil;' => "\u{162}",
        'Tcy;' => "\u{422}",
        'Tfr;' => "\u{1d517}",
        'Therefore;' => "\u{2234}",
        'Theta;' => "\u{398}",
        'ThickSpace;' => "\u{205f}\u{200a}",
        'ThinSpace;' => "\u{2009}",
        'Tilde;' => "\u{223c}",
        'TildeEqual;' => "\u{2243}",
        'TildeFullEqual;' => "\u{2245}",
        'TildeTilde;' => "\u{2248}",
        'Topf;' => "\u{1d54b}",
        'TripleDot;' => "\u{20db}",
        'Tscr;' => "\u{1d4af}",
        'Tstrok;' => "\u{166}",
        'Uacute;' => "\u{da}",
        'Uarr;' => "\u{219f}",
        'Uarrocir;' => "\u{2949}",
        'Ubrcy;' => "\u{40e}",
        'Ubreve;' => "\u{16c}",
        'Ucirc;' => "\u{db}",
        'Ucy;' => "\u{423}",
        'Udblac;' => "\u{170}",
        'Ufr;' => "\u{1d518}",
        'Ugrave;' => "\u{d9}",
        'Umacr;' => "\u{16a}",
        'UnderBar;' => "\u{5f}",
        'UnderBrace;' => "\u{23df}",
        'UnderBracket;' => "\u{23b5}",
        'UnderParenthesis;' => "\u{23dd}",
        'Union;' => "\u{22c3}",
        'UnionPlus;' => "\u{228e}",
        'Uogon;' => "\u{172}",
        'Uopf;' => "\u{1d54c}",
        'UpArrow;' => "\u{2191}",
        'UpArrowBar;' => "\u{2912}",
        'UpArrowDownArrow;' => "\u{21c5}",
        'UpDownArrow;' => "\u{2195}",
        'UpEquilibrium;' => "\u{296e}",
        'UpTee;' => "\u{22a5}",
        'UpTeeArrow;' => "\u{21a5}",
        'Uparrow;' => "\u{21d1}",
        'Updownarrow;' => "\u{21d5}",
        'UpperLeftArrow;' => "\u{2196}",
        'UpperRightArrow;' => "\u{2197}",
        'Upsi;' => "\u{3d2}",
        'Upsilon;' => "\u{3a5}",
        'Uring;' => "\u{16e}",
        'Uscr;' => "\u{1d4b0}",
        'Utilde;' => "\u{168}",
        'Uuml;' => "\u{dc}",
        'VDash;' => "\u{22ab}",
        'Vbar;' => "\u{2aeb}",
        'Vcy;' => "\u{412}",
        'Vdash;' => "\u{22a9}",
        'Vdashl;' => "\u{2ae6}",
        'Vee;' => "\u{22c1}",
        'Verbar;' => "\u{2016}",
        'Vert;' => "\u{2016}",
        'VerticalBar;' => "\u{2223}",
        'VerticalLine;' => "\u{7c}",
        'VerticalSeparator;' => "\u{2758}",
        'VerticalTilde;' => "\u{2240}",
        'VeryThinSpace;' => "\u{200a}",
        'Vfr;' => "\u{1d519}",
        'Vopf;' => "\u{1d54d}",
        'Vscr;' => "\u{1d4b1}",
        'Vvdash;' => "\u{22aa}",
        'Wcirc;' => "\u{174}",
        'Wedge;' => "\u{22c0}",
        'Wfr;' => "\u{1d51a}",
        'Wopf;' => "\u{1d54e}",
        'Wscr;' => "\u{1d4b2}",
        'Xfr;' => "\u{1d51b}",
        'Xi;' => "\u{39e}",
        'Xopf;' => "\u{1d54f}",
        'Xscr;' => "\u{1d4b3}",
        'YAcy;' => "\u{42f}",
        'YIcy;' => "\u{407}",
        'YUcy;' => "\u{42e}",
        'Yacute;' => "\u{dd}",
        'Ycirc;' => "\u{176}",
        'Ycy;' => "\u{42b}",
        'Yfr;' => "\u{1d51c}",
        'Yopf;' => "\u{1d550}",
        'Yscr;' => "\u{1d4b4}",
        'Yuml;' => "\u{178}",
        'ZHcy;' => "\u{416}",
        'Zacute;' => "\u{179}",
        'Zcaron;' => "\u{17d}",
        'Zcy;' => "\u{417}",
        'Zdot;' => "\u{17b}",
        'ZeroWidthSpace;' => "\u{200b}",
        'Zeta;' => "\u{396}",
        'Zfr;' => "\u{2128}",
        'Zopf;' => "\u{2124}",
        'Zscr;' => "\u{1d4b5}",
        'aacute;' => "\u{e1}",
        'abreve;' => "\u{103}",
        'ac;' => "\u{223e}",
        'acE;' => "\u{223e}\u{333}",
        'acd;' => "\u{223f}",
        'acirc;' => "\u{e2}",
        'acute;' => "\u{b4}",
        'acy;' => "\u{430}",
        'aelig;' => "\u{e6}",
        'af;' => "\u{2061}",
        'afr;' => "\u{1d51e}",
        'agrave;' => "\u{e0}",
        'alefsym;' => "\u{2135}",
        'aleph;' => "\u{2135}",
        'alpha;' => "\u{3b1}",
        'amacr;' => "\u{101}",
        'amalg;' => "\u{2a3f}",
        'amp;' => "\u{26}",
        'and;' => "\u{2227}",
        'andand;' => "\u{2a55}",
        'andd;' => "\u{2a5c}",
        'andslope;' => "\u{2a58}",
        'andv;' => "\u{2a5a}",
        'ang;' => "\u{2220}",
        'ange;' => "\u{29a4}",
        'angle;' => "\u{2220}",
        'angmsd;' => "\u{2221}",
        'angmsdaa;' => "\u{29a8}",
        'angmsdab;' => "\u{29a9}",
        'angmsdac;' => "\u{29aa}",
        'angmsdad;' => "\u{29ab}",
        'angmsdae;' => "\u{29ac}",
        'angmsdaf;' => "\u{29ad}",
        'angmsdag;' => "\u{29ae}",
        'angmsdah;' => "\u{29af}",
        'angrt;' => "\u{221f}",
        'angrtvb;' => "\u{22be}",
        'angrtvbd;' => "\u{299d}",
        'angsph;' => "\u{2222}",
        'angst;' => "\u{c5}",
        'angzarr;' => "\u{237c}",
        'aogon;' => "\u{105}",
        'aopf;' => "\u{1d552}",
        'ap;' => "\u{2248}",
        'apE;' => "\u{2a70}",
        'apacir;' => "\u{2a6f}",
        'ape;' => "\u{224a}",
        'apid;' => "\u{224b}",
        'apos;' => "\u{27}",
        'approx;' => "\u{2248}",
        'approxeq;' => "\u{224a}",
        'aring;' => "\u{e5}",
        'ascr;' => "\u{1d4b6}",
        'ast;' => "\u{2a}",
        'asymp;' => "\u{2248}",
        'asympeq;' => "\u{224d}",
        'atilde;' => "\u{e3}",
        'auml;' => "\u{e4}",
        'awconint;' => "\u{2233}",
        'awint;' => "\u{2a11}",
        'bNot;' => "\u{2aed}",
        'backcong;' => "\u{224c}",
        'backepsilon;' => "\u{3f6}",
        'backprime;' => "\u{2035}",
        'backsim;' => "\u{223d}",
        'backsimeq;' => "\u{22cd}",
        'barvee;' => "\u{22bd}",
        'barwed;' => "\u{2305}",
        'barwedge;' => "\u{2305}",
        'bbrk;' => "\u{23b5}",
        'bbrktbrk;' => "\u{23b6}",
        'bcong;' => "\u{224c}",
        'bcy;' => "\u{431}",
        'bdquo;' => "\u{201e}",
        'becaus;' => "\u{2235}",
        'because;' => "\u{2235}",
        'bemptyv;' => "\u{29b0}",
        'bepsi;' => "\u{3f6}",
        'bernou;' => "\u{212c}",
        'beta;' => "\u{3b2}",
        'beth;' => "\u{2136}",
        'between;' => "\u{226c}",
        'bfr;' => "\u{1d51f}",
        'bigcap;' => "\u{22c2}",
        'bigcirc;' => "\u{25ef}",
        'bigcup;' => "\u{22c3}",
        'bigodot;' => "\u{2a00}",
        'bigoplus;' => "\u{2a01}",
        'bigotimes;' => "\u{2a02}",
        'bigsqcup;' => "\u{2a06}",
        'bigstar;' => "\u{2605}",
        'bigtriangledown;' => "\u{25bd}",
        'bigtriangleup;' => "\u{25b3}",
        'biguplus;' => "\u{2a04}",
        'bigvee;' => "\u{22c1}",
        'bigwedge;' => "\u{22c0}",
        'bkarow;' => "\u{290d}",
        'blacklozenge;' => "\u{29eb}",
        'blacksquare;' => "\u{25aa}",
        'blacktriangle;' => "\u{25b4}",
        'blacktriangledown;' => "\u{25be}",
        'blacktriangleleft;' => "\u{25c2}",
        'blacktriangleright;' => "\u{25b8}",
        'blank;' => "\u{2423}",
        'blk12;' => "\u{2592}",
        'blk14;' => "\u{2591}",
        'blk34;' => "\u{2593}",
        'block;' => "\u{2588}",
        'bne;' => "\u{3d}\u{20e5}",
        'bnequiv;' => "\u{2261}\u{20e5}",
        'bnot;' => "\u{2310}",
        'bopf;' => "\u{1d553}",
        'bot;' => "\u{22a5}",
        'bottom;' => "\u{22a5}",
        'bowtie;' => "\u{22c8}",
        'boxDL;' => "\u{2557}",
        'boxDR;' => "\u{2554}",
        'boxDl;' => "\u{2556}",
        'boxDr;' => "\u{2553}",
        'boxH;' => "\u{2550}",
        'boxHD;' => "\u{2566}",
        'boxHU;' => "\u{2569}",
        'boxHd;' => "\u{2564}",
        'boxHu;' => "\u{2567}",
        'boxUL;' => "\u{255d}",
        'boxUR;' => "\u{255a}",
        'boxUl;' => "\u{255c}",
        'boxUr;' => "\u{2559}",
        'boxV;' => "\u{2551}",
        'boxVH;' => "\u{256c}",
        'boxVL;' => "\u{2563}",
        'boxVR;' => "\u{2560}",
        'boxVh;' => "\u{256b}",
        'boxVl;' => "\u{2562}",
        'boxVr;' => "\u{255f}",
        'boxbox;' => "\u{29c9}",
        'boxdL;' => "\u{2555}",
        'boxdR;' => "\u{2552}",
        'boxdl;' => "\u{2510}",
        'boxdr;' => "\u{250c}",
        'boxh;' => "\u{2500}",
        'boxhD;' => "\u{2565}",
        'boxhU;' => "\u{2568}",
        'boxhd;' => "\u{252c}",
        'boxhu;' => "\u{2534}",
        'boxminus;' => "\u{229f}",
        'boxplus;' => "\u{229e}",
        'boxtimes;' => "\u{22a0}",
        'boxuL;' => "\u{255b}",
        'boxuR;' => "\u{2558}",
        'boxul;' => "\u{2518}",
        'boxur;' => "\u{2514}",
        'boxv;' => "\u{2502}",
        'boxvH;' => "\u{256a}",
        'boxvL;' => "\u{2561}",
        'boxvR;' => "\u{255e}",
        'boxvh;' => "\u{253c}",
        'boxvl;' => "\u{2524}",
        'boxvr;' => "\u{251c}",
        'bprime;' => "\u{2035}",
        'breve;' => "\u{2d8}",
        'brvbar;' => "\u{a6}",
        'bscr;' => "\u{1d4b7}",
        'bsemi;' => "\u{204f}",
        'bsim;' => "\u{223d}",
        'bsime;' => "\u{22cd}",
        'bsol;' => "\u{5c}",
        'bsolb;' => "\u{29c5}",
        'bsolhsub;' => "\u{27c8}",
        'bull;' => "\u{2022}",
        'bullet;' => "\u{2022}",
        'bump;' => "\u{224e}",
        'bumpE;' => "\u{2aae}",
        'bumpe;' => "\u{224f}",
        'bumpeq;' => "\u{224f}",
        'cacute;' => "\u{107}",
        'cap;' => "\u{2229}",
        'capand;' => "\u{2a44}",
        'capbrcup;' => "\u{2a49}",
        'capcap;' => "\u{2a4b}",
        'capcup;' => "\u{2a47}",
        'capdot;' => "\u{2a40}",
        'caps;' => "\u{2229}\u{fe00}",
        'caret;' => "\u{2041}",
        'caron;' => "\u{2c7}",
        'ccaps;' => "\u{2a4d}",
        'ccaron;' => "\u{10d}",
        'ccedil;' => "\u{e7}",
        'ccirc;' => "\u{109}",
        'ccups;' => "\u{2a4c}",
        'ccupssm;' => "\u{2a50}",
        'cdot;' => "\u{10b}",
        'cedil;' => "\u{b8}",
        'cemptyv;' => "\u{29b2}",
        'cent;' => "\u{a2}",
        'centerdot;' => "\u{b7}",
        'cfr;' => "\u{1d520}",
        'chcy;' => "\u{447}",
        'check;' => "\u{2713}",
        'checkmark;' => "\u{2713}",
        'chi;' => "\u{3c7}",
        'cir;' => "\u{25cb}",
        'cirE;' => "\u{29c3}",
        'circ;' => "\u{2c6}",
        'circeq;' => "\u{2257}",
        'circlearrowleft;' => "\u{21ba}",
        'circlearrowright;' => "\u{21bb}",
        'circledR;' => "\u{ae}",
        'circledS;' => "\u{24c8}",
        'circledast;' => "\u{229b}",
        'circledcirc;' => "\u{229a}",
        'circleddash;' => "\u{229d}",
        'cire;' => "\u{2257}",
        'cirfnint;' => "\u{2a10}",
        'cirmid;' => "\u{2aef}",
        'cirscir;' => "\u{29c2}",
        'clubs;' => "\u{2663}",
        'clubsuit;' => "\u{2663}",
        'colon;' => "\u{3a}",
        'colone;' => "\u{2254}",
        'coloneq;' => "\u{2254}",
        'comma;' => "\u{2c}",
        'commat;' => "\u{40}",
        'comp;' => "\u{2201}",
        'compfn;' => "\u{2218}",
        'complement;' => "\u{2201}",
        'complexes;' => "\u{2102}",
        'cong;' => "\u{2245}",
        'congdot;' => "\u{2a6d}",
        'conint;' => "\u{222e}",
        'copf;' => "\u{1d554}",
        'coprod;' => "\u{2210}",
        'copy;' => "\u{a9}",
        'copysr;' => "\u{2117}",
        'crarr;' => "\u{21b5}",
        'cross;' => "\u{2717}",
        'cscr;' => "\u{1d4b8}",
        'csub;' => "\u{2acf}",
        'csube;' => "\u{2ad1}",
        'csup;' => "\u{2ad0}",
        'csupe;' => "\u{2ad2}",
        'ctdot;' => "\u{22ef}",
        'cudarrl;' => "\u{2938}",
        'cudarrr;' => "\u{2935}",
        'cuepr;' => "\u{22de}",
        'cuesc;' => "\u{22df}",
        'cularr;' => "\u{21b6}",
        'cularrp;' => "\u{293d}",
        'cup;' => "\u{222a}",
        'cupbrcap;' => "\u{2a48}",
        'cupcap;' => "\u{2a46}",
        'cupcup;' => "\u{2a4a}",
        'cupdot;' => "\u{228d}",
        'cupor;' => "\u{2a45}",
        'cups;' => "\u{222a}\u{fe00}",
        'curarr;' => "\u{21b7}",
        'curarrm;' => "\u{293c}",
        'curlyeqprec;' => "\u{22de}",
        'curlyeqsucc;' => "\u{22df}",
        'curlyvee;' => "\u{22ce}",
        'curlywedge;' => "\u{22cf}",
        'curren;' => "\u{a4}",
        'curvearrowleft;' => "\u{21b6}",
        'curvearrowright;' => "\u{21b7}",
        'cuvee;' => "\u{22ce}",
        'cuwed;' => "\u{22cf}",
        'cwconint;' => "\u{2232}",
        'cwint;' => "\u{2231}",
        'cylcty;' => "\u{232d}",
        'dArr;' => "\u{21d3}",
        'dHar;' => "\u{2965}",
        'dagger;' => "\u{2020}",
        'daleth;' => "\u{2138}",
        'darr;' => "\u{2193}",
        'dash;' => "\u{2010}",
        'dashv;' => "\u{22a3}",
        'dbkarow;' => "\u{290f}",
        'dblac;' => "\u{2dd}",
        'dcaron;' => "\u{10f}",
        'dcy;' => "\u{434}",
        'dd;' => "\u{2146}",
        'ddagger;' => "\u{2021}",
        'ddarr;' => "\u{21ca}",
        'ddotseq;' => "\u{2a77}",
        'deg;' => "\u{b0}",
        'delta;' => "\u{3b4}",
        'demptyv;' => "\u{29b1}",
        'dfisht;' => "\u{297f}",
        'dfr;' => "\u{1d521}",
        'dharl;' => "\u{21c3}",
        'dharr;' => "\u{21c2}",
        'diam;' => "\u{22c4}",
        'diamond;' => "\u{22c4}",
        'diamondsuit;' => "\u{2666}",
        'diams;' => "\u{2666}",
        'die;' => "\u{a8}",
        'digamma;' => "\u{3dd}",
        'disin;' => "\u{22f2}",
        'div;' => "\u{f7}",
        'divide;' => "\u{f7}",
        'divideontimes;' => "\u{22c7}",
        'divonx;' => "\u{22c7}",
        'djcy;' => "\u{452}",
        'dlcorn;' => "\u{231e}",
        'dlcrop;' => "\u{230d}",
        'dollar;' => "\u{24}",
        'dopf;' => "\u{1d555}",
        'dot;' => "\u{2d9}",
        'doteq;' => "\u{2250}",
        'doteqdot;' => "\u{2251}",
        'dotminus;' => "\u{2238}",
        'dotplus;' => "\u{2214}",
        'dotsquare;' => "\u{22a1}",
        'doublebarwedge;' => "\u{2306}",
        'downarrow;' => "\u{2193}",
        'downdownarrows;' => "\u{21ca}",
        'downharpoonleft;' => "\u{21c3}",
        'downharpoonright;' => "\u{21c2}",
        'drbkarow;' => "\u{2910}",
        'drcorn;' => "\u{231f}",
        'drcrop;' => "\u{230c}",
        'dscr;' => "\u{1d4b9}",
        'dscy;' => "\u{455}",
        'dsol;' => "\u{29f6}",
        'dstrok;' => "\u{111}",
        'dtdot;' => "\u{22f1}",
        'dtri;' => "\u{25bf}",
        'dtrif;' => "\u{25be}",
        'duarr;' => "\u{21f5}",
        'duhar;' => "\u{296f}",
        'dwangle;' => "\u{29a6}",
        'dzcy;' => "\u{45f}",
        'dzigrarr;' => "\u{27ff}",
        'eDDot;' => "\u{2a77}",
        'eDot;' => "\u{2251}",
        'eacute;' => "\u{e9}",
        'easter;' => "\u{2a6e}",
        'ecaron;' => "\u{11b}",
        'ecir;' => "\u{2256}",
        'ecirc;' => "\u{ea}",
        'ecolon;' => "\u{2255}",
        'ecy;' => "\u{44d}",
        'edot;' => "\u{117}",
        'ee;' => "\u{2147}",
        'efDot;' => "\u{2252}",
        'efr;' => "\u{1d522}",
        'eg;' => "\u{2a9a}",
        'egrave;' => "\u{e8}",
        'egs;' => "\u{2a96}",
        'egsdot;' => "\u{2a98}",
        'el;' => "\u{2a99}",
        'elinters;' => "\u{23e7}",
        'ell;' => "\u{2113}",
        'els;' => "\u{2a95}",
        'elsdot;' => "\u{2a97}",
        'emacr;' => "\u{113}",
        'empty;' => "\u{2205}",
        'emptyset;' => "\u{2205}",
        'emptyv;' => "\u{2205}",
        'emsp13;' => "\u{2004}",
        'emsp14;' => "\u{2005}",
        'emsp;' => "\u{2003}",
        'eng;' => "\u{14b}",
        'ensp;' => "\u{2002}",
        'eogon;' => "\u{119}",
        'eopf;' => "\u{1d556}",
        'epar;' => "\u{22d5}",
        'eparsl;' => "\u{29e3}",
        'eplus;' => "\u{2a71}",
        'epsi;' => "\u{3b5}",
        'epsilon;' => "\u{3b5}",
        'epsiv;' => "\u{3f5}",
        'eqcirc;' => "\u{2256}",
        'eqcolon;' => "\u{2255}",
        'eqsim;' => "\u{2242}",
        'eqslantgtr;' => "\u{2a96}",
        'eqslantless;' => "\u{2a95}",
        'equals;' => "\u{3d}",
        'equest;' => "\u{225f}",
        'equiv;' => "\u{2261}",
        'equivDD;' => "\u{2a78}",
        'eqvparsl;' => "\u{29e5}",
        'erDot;' => "\u{2253}",
        'erarr;' => "\u{2971}",
        'escr;' => "\u{212f}",
        'esdot;' => "\u{2250}",
        'esim;' => "\u{2242}",
        'eta;' => "\u{3b7}",
        'eth;' => "\u{f0}",
        'euml;' => "\u{eb}",
        'euro;' => "\u{20ac}",
        'excl;' => "\u{21}",
        'exist;' => "\u{2203}",
        'expectation;' => "\u{2130}",
        'exponentiale;' => "\u{2147}",
        'fallingdotseq;' => "\u{2252}",
        'fcy;' => "\u{444}",
        'female;' => "\u{2640}",
        'ffilig;' => "\u{fb03}",
        'fflig;' => "\u{fb00}",
        'ffllig;' => "\u{fb04}",
        'ffr;' => "\u{1d523}",
        'filig;' => "\u{fb01}",
        'fjlig;' => "\u{66}\u{6a}",
        'flat;' => "\u{266d}",
        'fllig;' => "\u{fb02}",
        'fltns;' => "\u{25b1}",
        'fnof;' => "\u{192}",
        'fopf;' => "\u{1d557}",
        'forall;' => "\u{2200}",
        'fork;' => "\u{22d4}",
        'forkv;' => "\u{2ad9}",
        'fpartint;' => "\u{2a0d}",
        'frac12;' => "\u{bd}",
        'frac13;' => "\u{2153}",
        'frac14;' => "\u{bc}",
        'frac15;' => "\u{2155}",
        'frac16;' => "\u{2159}",
        'frac18;' => "\u{215b}",
        'frac23;' => "\u{2154}",
        'frac25;' => "\u{2156}",
        'frac34;' => "\u{be}",
        'frac35;' => "\u{2157}",
        'frac38;' => "\u{215c}",
        'frac45;' => "\u{2158}",
        'frac56;' => "\u{215a}",
        'frac58;' => "\u{215d}",
        'frac78;' => "\u{215e}",
        'frasl;' => "\u{2044}",
        'frown;' => "\u{2322}",
        'fscr;' => "\u{1d4bb}",
        'gE;' => "\u{2267}",
        'gEl;' => "\u{2a8c}",
        'gacute;' => "\u{1f5}",
        'gamma;' => "\u{3b3}",
        'gammad;' => "\u{3dd}",
        'gap;' => "\u{2a86}",
        'gbreve;' => "\u{11f}",
        'gcirc;' => "\u{11d}",
        'gcy;' => "\u{433}",
        'gdot;' => "\u{121}",
        'ge;' => "\u{2265}",
        'gel;' => "\u{22db}",
        'geq;' => "\u{2265}",
        'geqq;' => "\u{2267}",
        'geqslant;' => "\u{2a7e}",
        'ges;' => "\u{2a7e}",
        'gescc;' => "\u{2aa9}",
        'gesdot;' => "\u{2a80}",
        'gesdoto;' => "\u{2a82}",
        'gesdotol;' => "\u{2a84}",
        'gesl;' => "\u{22db}\u{fe00}",
        'gesles;' => "\u{2a94}",
        'gfr;' => "\u{1d524}",
        'gg;' => "\u{226b}",
        'ggg;' => "\u{22d9}",
        'gimel;' => "\u{2137}",
        'gjcy;' => "\u{453}",
        'gl;' => "\u{2277}",
        'glE;' => "\u{2a92}",
        'gla;' => "\u{2aa5}",
        'glj;' => "\u{2aa4}",
        'gnE;' => "\u{2269}",
        'gnap;' => "\u{2a8a}",
        'gnapprox;' => "\u{2a8a}",
        'gne;' => "\u{2a88}",
        'gneq;' => "\u{2a88}",
        'gneqq;' => "\u{2269}",
        'gnsim;' => "\u{22e7}",
        'gopf;' => "\u{1d558}",
        'grave;' => "\u{60}",
        'gscr;' => "\u{210a}",
        'gsim;' => "\u{2273}",
        'gsime;' => "\u{2a8e}",
        'gsiml;' => "\u{2a90}",
        'gt;' => "\u{3e}",
        'gtcc;' => "\u{2aa7}",
        'gtcir;' => "\u{2a7a}",
        'gtdot;' => "\u{22d7}",
        'gtlPar;' => "\u{2995}",
        'gtquest;' => "\u{2a7c}",
        'gtrapprox;' => "\u{2a86}",
        'gtrarr;' => "\u{2978}",
        'gtrdot;' => "\u{22d7}",
        'gtreqless;' => "\u{22db}",
        'gtreqqless;' => "\u{2a8c}",
        'gtrless;' => "\u{2277}",
        'gtrsim;' => "\u{2273}",
        'gvertneqq;' => "\u{2269}\u{fe00}",
        'gvnE;' => "\u{2269}\u{fe00}",
        'hArr;' => "\u{21d4}",
        'hairsp;' => "\u{200a}",
        'half;' => "\u{bd}",
        'hamilt;' => "\u{210b}",
        'hardcy;' => "\u{44a}",
        'harr;' => "\u{2194}",
        'harrcir;' => "\u{2948}",
        'harrw;' => "\u{21ad}",
        'hbar;' => "\u{210f}",
        'hcirc;' => "\u{125}",
        'hearts;' => "\u{2665}",
        'heartsuit;' => "\u{2665}",
        'hellip;' => "\u{2026}",
        'hercon;' => "\u{22b9}",
        'hfr;' => "\u{1d525}",
        'hksearow;' => "\u{2925}",
        'hkswarow;' => "\u{2926}",
        'hoarr;' => "\u{21ff}",
        'homtht;' => "\u{223b}",
        'hookleftarrow;' => "\u{21a9}",
        'hookrightarrow;' => "\u{21aa}",
        'hopf;' => "\u{1d559}",
        'horbar;' => "\u{2015}",
        'hscr;' => "\u{1d4bd}",
        'hslash;' => "\u{210f}",
        'hstrok;' => "\u{127}",
        'hybull;' => "\u{2043}",
        'hyphen;' => "\u{2010}",
        'iacute;' => "\u{ed}",
        'ic;' => "\u{2063}",
        'icirc;' => "\u{ee}",
        'icy;' => "\u{438}",
        'iecy;' => "\u{435}",
        'iexcl;' => "\u{a1}",
        'iff;' => "\u{21d4}",
        'ifr;' => "\u{1d526}",
        'igrave;' => "\u{ec}",
        'ii;' => "\u{2148}",
        'iiiint;' => "\u{2a0c}",
        'iiint;' => "\u{222d}",
        'iinfin;' => "\u{29dc}",
        'iiota;' => "\u{2129}",
        'ijlig;' => "\u{133}",
        'imacr;' => "\u{12b}",
        'image;' => "\u{2111}",
        'imagline;' => "\u{2110}",
        'imagpart;' => "\u{2111}",
        'imath;' => "\u{131}",
        'imof;' => "\u{22b7}",
        'imped;' => "\u{1b5}",
        'in;' => "\u{2208}",
        'incare;' => "\u{2105}",
        'infin;' => "\u{221e}",
        'infintie;' => "\u{29dd}",
        'inodot;' => "\u{131}",
        'int;' => "\u{222b}",
        'intcal;' => "\u{22ba}",
        'integers;' => "\u{2124}",
        'intercal;' => "\u{22ba}",
        'intlarhk;' => "\u{2a17}",
        'intprod;' => "\u{2a3c}",
        'iocy;' => "\u{451}",
        'iogon;' => "\u{12f}",
        'iopf;' => "\u{1d55a}",
        'iota;' => "\u{3b9}",
        'iprod;' => "\u{2a3c}",
        'iquest;' => "\u{bf}",
        'iscr;' => "\u{1d4be}",
        'isin;' => "\u{2208}",
        'isinE;' => "\u{22f9}",
        'isindot;' => "\u{22f5}",
        'isins;' => "\u{22f4}",
        'isinsv;' => "\u{22f3}",
        'isinv;' => "\u{2208}",
        'it;' => "\u{2062}",
        'itilde;' => "\u{129}",
        'iukcy;' => "\u{456}",
        'iuml;' => "\u{ef}",
        'jcirc;' => "\u{135}",
        'jcy;' => "\u{439}",
        'jfr;' => "\u{1d527}",
        'jmath;' => "\u{237}",
        'jopf;' => "\u{1d55b}",
        'jscr;' => "\u{1d4bf}",
        'jsercy;' => "\u{458}",
        'jukcy;' => "\u{454}",
        'kappa;' => "\u{3ba}",
        'kappav;' => "\u{3f0}",
        'kcedil;' => "\u{137}",
        'kcy;' => "\u{43a}",
        'kfr;' => "\u{1d528}",
        'kgreen;' => "\u{138}",
        'khcy;' => "\u{445}",
        'kjcy;' => "\u{45c}",
        'kopf;' => "\u{1d55c}",
        'kscr;' => "\u{1d4c0}",
        'lAarr;' => "\u{21da}",
        'lArr;' => "\u{21d0}",
        'lAtail;' => "\u{291b}",
        'lBarr;' => "\u{290e}",
        'lE;' => "\u{2266}",
        'lEg;' => "\u{2a8b}",
        'lHar;' => "\u{2962}",
        'lacute;' => "\u{13a}",
        'laemptyv;' => "\u{29b4}",
        'lagran;' => "\u{2112}",
        'lambda;' => "\u{3bb}",
        'lang;' => "\u{27e8}",
        'langd;' => "\u{2991}",
        'langle;' => "\u{27e8}",
        'lap;' => "\u{2a85}",
        'laquo;' => "\u{ab}",
        'larr;' => "\u{2190}",
        'larrb;' => "\u{21e4}",
        'larrbfs;' => "\u{291f}",
        'larrfs;' => "\u{291d}",
        'larrhk;' => "\u{21a9}",
        'larrlp;' => "\u{21ab}",
        'larrpl;' => "\u{2939}",
        'larrsim;' => "\u{2973}",
        'larrtl;' => "\u{21a2}",
        'lat;' => "\u{2aab}",
        'latail;' => "\u{2919}",
        'late;' => "\u{2aad}",
        'lates;' => "\u{2aad}\u{fe00}",
        'lbarr;' => "\u{290c}",
        'lbbrk;' => "\u{2772}",
        'lbrace;' => "\u{7b}",
        'lbrack;' => "\u{5b}",
        'lbrke;' => "\u{298b}",
        'lbrksld;' => "\u{298f}",
        'lbrkslu;' => "\u{298d}",
        'lcaron;' => "\u{13e}",
        'lcedil;' => "\u{13c}",
        'lceil;' => "\u{2308}",
        'lcub;' => "\u{7b}",
        'lcy;' => "\u{43b}",
        'ldca;' => "\u{2936}",
        'ldquo;' => "\u{201c}",
        'ldquor;' => "\u{201e}",
        'ldrdhar;' => "\u{2967}",
        'ldrushar;' => "\u{294b}",
        'ldsh;' => "\u{21b2}",
        'le;' => "\u{2264}",
        'leftarrow;' => "\u{2190}",
        'leftarrowtail;' => "\u{21a2}",
        'leftharpoondown;' => "\u{21bd}",
        'leftharpoonup;' => "\u{21bc}",
        'leftleftarrows;' => "\u{21c7}",
        'leftrightarrow;' => "\u{2194}",
        'leftrightarrows;' => "\u{21c6}",
        'leftrightharpoons;' => "\u{21cb}",
        'leftrightsquigarrow;' => "\u{21ad}",
        'leftthreetimes;' => "\u{22cb}",
        'leg;' => "\u{22da}",
        'leq;' => "\u{2264}",
        'leqq;' => "\u{2266}",
        'leqslant;' => "\u{2a7d}",
        'les;' => "\u{2a7d}",
        'lescc;' => "\u{2aa8}",
        'lesdot;' => "\u{2a7f}",
        'lesdoto;' => "\u{2a81}",
        'lesdotor;' => "\u{2a83}",
        'lesg;' => "\u{22da}\u{fe00}",
        'lesges;' => "\u{2a93}",
        'lessapprox;' => "\u{2a85}",
        'lessdot;' => "\u{22d6}",
        'lesseqgtr;' => "\u{22da}",
        'lesseqqgtr;' => "\u{2a8b}",
        'lessgtr;' => "\u{2276}",
        'lesssim;' => "\u{2272}",
        'lfisht;' => "\u{297c}",
        'lfloor;' => "\u{230a}",
        'lfr;' => "\u{1d529}",
        'lg;' => "\u{2276}",
        'lgE;' => "\u{2a91}",
        'lhard;' => "\u{21bd}",
        'lharu;' => "\u{21bc}",
        'lharul;' => "\u{296a}",
        'lhblk;' => "\u{2584}",
        'ljcy;' => "\u{459}",
        'll;' => "\u{226a}",
        'llarr;' => "\u{21c7}",
        'llcorner;' => "\u{231e}",
        'llhard;' => "\u{296b}",
        'lltri;' => "\u{25fa}",
        'lmidot;' => "\u{140}",
        'lmoust;' => "\u{23b0}",
        'lmoustache;' => "\u{23b0}",
        'lnE;' => "\u{2268}",
        'lnap;' => "\u{2a89}",
        'lnapprox;' => "\u{2a89}",
        'lne;' => "\u{2a87}",
        'lneq;' => "\u{2a87}",
        'lneqq;' => "\u{2268}",
        'lnsim;' => "\u{22e6}",
        'loang;' => "\u{27ec}",
        'loarr;' => "\u{21fd}",
        'lobrk;' => "\u{27e6}",
        'longleftarrow;' => "\u{27f5}",
        'longleftrightarrow;' => "\u{27f7}",
        'longmapsto;' => "\u{27fc}",
        'longrightarrow;' => "\u{27f6}",
        'looparrowleft;' => "\u{21ab}",
        'looparrowright;' => "\u{21ac}",
        'lopar;' => "\u{2985}",
        'lopf;' => "\u{1d55d}",
        'loplus;' => "\u{2a2d}",
        'lotimes;' => "\u{2a34}",
        'lowast;' => "\u{2217}",
        'lowbar;' => "\u{5f}",
        'loz;' => "\u{25ca}",
        'lozenge;' => "\u{25ca}",
        'lozf;' => "\u{29eb}",
        'lpar;' => "\u{28}",
        'lparlt;' => "\u{2993}",
        'lrarr;' => "\u{21c6}",
        'lrcorner;' => "\u{231f}",
        'lrhar;' => "\u{21cb}",
        'lrhard;' => "\u{296d}",
        'lrm;' => "\u{200e}",
        'lrtri;' => "\u{22bf}",
        'lsaquo;' => "\u{2039}",
        'lscr;' => "\u{1d4c1}",
        'lsh;' => "\u{21b0}",
        'lsim;' => "\u{2272}",
        'lsime;' => "\u{2a8d}",
        'lsimg;' => "\u{2a8f}",
        'lsqb;' => "\u{5b}",
        'lsquo;' => "\u{2018}",
        'lsquor;' => "\u{201a}",
        'lstrok;' => "\u{142}",
        'lt;' => "\u{3c}",
        'ltcc;' => "\u{2aa6}",
        'ltcir;' => "\u{2a79}",
        'ltdot;' => "\u{22d6}",
        'lthree;' => "\u{22cb}",
        'ltimes;' => "\u{22c9}",
        'ltlarr;' => "\u{2976}",
        'ltquest;' => "\u{2a7b}",
        'ltrPar;' => "\u{2996}",
        'ltri;' => "\u{25c3}",
        'ltrie;' => "\u{22b4}",
        'ltrif;' => "\u{25c2}",
        'lurdshar;' => "\u{294a}",
        'luruhar;' => "\u{2966}",
        'lvertneqq;' => "\u{2268}\u{fe00}",
        'lvnE;' => "\u{2268}\u{fe00}",
        'mDDot;' => "\u{223a}",
        'macr;' => "\u{af}",
        'male;' => "\u{2642}",
        'malt;' => "\u{2720}",
        'maltese;' => "\u{2720}",
        'map;' => "\u{21a6}",
        'mapsto;' => "\u{21a6}",
        'mapstodown;' => "\u{21a7}",
        'mapstoleft;' => "\u{21a4}",
        'mapstoup;' => "\u{21a5}",
        'marker;' => "\u{25ae}",
        'mcomma;' => "\u{2a29}",
        'mcy;' => "\u{43c}",
        'mdash;' => "\u{2014}",
        'measuredangle;' => "\u{2221}",
        'mfr;' => "\u{1d52a}",
        'mho;' => "\u{2127}",
        'micro;' => "\u{b5}",
        'mid;' => "\u{2223}",
        'midast;' => "\u{2a}",
        'midcir;' => "\u{2af0}",
        'middot;' => "\u{b7}",
        'minus;' => "\u{2212}",
        'minusb;' => "\u{229f}",
        'minusd;' => "\u{2238}",
        'minusdu;' => "\u{2a2a}",
        'mlcp;' => "\u{2adb}",
        'mldr;' => "\u{2026}",
        'mnplus;' => "\u{2213}",
        'models;' => "\u{22a7}",
        'mopf;' => "\u{1d55e}",
        'mp;' => "\u{2213}",
        'mscr;' => "\u{1d4c2}",
        'mstpos;' => "\u{223e}",
        'mu;' => "\u{3bc}",
        'multimap;' => "\u{22b8}",
        'mumap;' => "\u{22b8}",
        'nGg;' => "\u{22d9}\u{338}",
        'nGt;' => "\u{226b}\u{20d2}",
        'nGtv;' => "\u{226b}\u{338}",
        'nLeftarrow;' => "\u{21cd}",
        'nLeftrightarrow;' => "\u{21ce}",
        'nLl;' => "\u{22d8}\u{338}",
        'nLt;' => "\u{226a}\u{20d2}",
        'nLtv;' => "\u{226a}\u{338}",
        'nRightarrow;' => "\u{21cf}",
        'nVDash;' => "\u{22af}",
        'nVdash;' => "\u{22ae}",
        'nabla;' => "\u{2207}",
        'nacute;' => "\u{144}",
        'nang;' => "\u{2220}\u{20d2}",
        'nap;' => "\u{2249}",
        'napE;' => "\u{2a70}\u{338}",
        'napid;' => "\u{224b}\u{338}",
        'napos;' => "\u{149}",
        'napprox;' => "\u{2249}",
        'natur;' => "\u{266e}",
        'natural;' => "\u{266e}",
        'naturals;' => "\u{2115}",
        'nbsp;' => "\u{a0}",
        'nbump;' => "\u{224e}\u{338}",
        'nbumpe;' => "\u{224f}\u{338}",
        'ncap;' => "\u{2a43}",
        'ncaron;' => "\u{148}",
        'ncedil;' => "\u{146}",
        'ncong;' => "\u{2247}",
        'ncongdot;' => "\u{2a6d}\u{338}",
        'ncup;' => "\u{2a42}",
        'ncy;' => "\u{43d}",
        'ndash;' => "\u{2013}",
        'ne;' => "\u{2260}",
        'neArr;' => "\u{21d7}",
        'nearhk;' => "\u{2924}",
        'nearr;' => "\u{2197}",
        'nearrow;' => "\u{2197}",
        'nedot;' => "\u{2250}\u{338}",
        'nequiv;' => "\u{2262}",
        'nesear;' => "\u{2928}",
        'nesim;' => "\u{2242}\u{338}",
        'nexist;' => "\u{2204}",
        'nexists;' => "\u{2204}",
        'nfr;' => "\u{1d52b}",
        'ngE;' => "\u{2267}\u{338}",
        'nge;' => "\u{2271}",
        'ngeq;' => "\u{2271}",
        'ngeqq;' => "\u{2267}\u{338}",
        'ngeqslant;' => "\u{2a7e}\u{338}",
        'nges;' => "\u{2a7e}\u{338}",
        'ngsim;' => "\u{2275}",
        'ngt;' => "\u{226f}",
        'ngtr;' => "\u{226f}",
        'nhArr;' => "\u{21ce}",
        'nharr;' => "\u{21ae}",
        'nhpar;' => "\u{2af2}",
        'ni;' => "\u{220b}",
        'nis;' => "\u{22fc}",
        'nisd;' => "\u{22fa}",
        'niv;' => "\u{220b}",
        'njcy;' => "\u{45a}",
        'nlArr;' => "\u{21cd}",
        'nlE;' => "\u{2266}\u{338}",
        'nlarr;' => "\u{219a}",
        'nldr;' => "\u{2025}",
        'nle;' => "\u{2270}",
        'nleftarrow;' => "\u{219a}",
        'nleftrightarrow;' => "\u{21ae}",
        'nleq;' => "\u{2270}",
        'nleqq;' => "\u{2266}\u{338}",
        'nleqslant;' => "\u{2a7d}\u{338}",
        'nles;' => "\u{2a7d}\u{338}",
        'nless;' => "\u{226e}",
        'nlsim;' => "\u{2274}",
        'nlt;' => "\u{226e}",
        'nltri;' => "\u{22ea}",
        'nltrie;' => "\u{22ec}",
        'nmid;' => "\u{2224}",
        'nopf;' => "\u{1d55f}",
        'not;' => "\u{ac}",
        'notin;' => "\u{2209}",
        'notinE;' => "\u{22f9}\u{338}",
        'notindot;' => "\u{22f5}\u{338}",
        'notinva;' => "\u{2209}",
        'notinvb;' => "\u{22f7}",
        'notinvc;' => "\u{22f6}",
        'notni;' => "\u{220c}",
        'notniva;' => "\u{220c}",
        'notnivb;' => "\u{22fe}",
        'notnivc;' => "\u{22fd}",
        'npar;' => "\u{2226}",
        'nparallel;' => "\u{2226}",
        'nparsl;' => "\u{2afd}\u{20e5}",
        'npart;' => "\u{2202}\u{338}",
        'npolint;' => "\u{2a14}",
        'npr;' => "\u{2280}",
        'nprcue;' => "\u{22e0}",
        'npre;' => "\u{2aaf}\u{338}",
        'nprec;' => "\u{2280}",
        'npreceq;' => "\u{2aaf}\u{338}",
        'nrArr;' => "\u{21cf}",
        'nrarr;' => "\u{219b}",
        'nrarrc;' => "\u{2933}\u{338}",
        'nrarrw;' => "\u{219d}\u{338}",
        'nrightarrow;' => "\u{219b}",
        'nrtri;' => "\u{22eb}",
        'nrtrie;' => "\u{22ed}",
        'nsc;' => "\u{2281}",
        'nsccue;' => "\u{22e1}",
        'nsce;' => "\u{2ab0}\u{338}",
        'nscr;' => "\u{1d4c3}",
        'nshortmid;' => "\u{2224}",
        'nshortparallel;' => "\u{2226}",
        'nsim;' => "\u{2241}",
        'nsime;' => "\u{2244}",
        'nsimeq;' => "\u{2244}",
        'nsmid;' => "\u{2224}",
        'nspar;' => "\u{2226}",
        'nsqsube;' => "\u{22e2}",
        'nsqsupe;' => "\u{22e3}",
        'nsub;' => "\u{2284}",
        'nsubE;' => "\u{2ac5}\u{338}",
        'nsube;' => "\u{2288}",
        'nsubset;' => "\u{2282}\u{20d2}",
        'nsubseteq;' => "\u{2288}",
        'nsubseteqq;' => "\u{2ac5}\u{338}",
        'nsucc;' => "\u{2281}",
        'nsucceq;' => "\u{2ab0}\u{338}",
        'nsup;' => "\u{2285}",
        'nsupE;' => "\u{2ac6}\u{338}",
        'nsupe;' => "\u{2289}",
        'nsupset;' => "\u{2283}\u{20d2}",
        'nsupseteq;' => "\u{2289}",
        'nsupseteqq;' => "\u{2ac6}\u{338}",
        'ntgl;' => "\u{2279}",
        'ntilde;' => "\u{f1}",
        'ntlg;' => "\u{2278}",
        'ntriangleleft;' => "\u{22ea}",
        'ntrianglelefteq;' => "\u{22ec}",
        'ntriangleright;' => "\u{22eb}",
        'ntrianglerighteq;' => "\u{22ed}",
        'nu;' => "\u{3bd}",
        'num;' => "\u{23}",
        'numero;' => "\u{2116}",
        'numsp;' => "\u{2007}",
        'nvDash;' => "\u{22ad}",
        'nvHarr;' => "\u{2904}",
        'nvap;' => "\u{224d}\u{20d2}",
        'nvdash;' => "\u{22ac}",
        'nvge;' => "\u{2265}\u{20d2}",
        'nvgt;' => "\u{3e}\u{20d2}",
        'nvinfin;' => "\u{29de}",
        'nvlArr;' => "\u{2902}",
        'nvle;' => "\u{2264}\u{20d2}",
        'nvlt;' => "\u{3c}\u{20d2}",
        'nvltrie;' => "\u{22b4}\u{20d2}",
        'nvrArr;' => "\u{2903}",
        'nvrtrie;' => "\u{22b5}\u{20d2}",
        'nvsim;' => "\u{223c}\u{20d2}",
        'nwArr;' => "\u{21d6}",
        'nwarhk;' => "\u{2923}",
        'nwarr;' => "\u{2196}",
        'nwarrow;' => "\u{2196}",
        'nwnear;' => "\u{2927}",
        'oS;' => "\u{24c8}",
        'oacute;' => "\u{f3}",
        'oast;' => "\u{229b}",
        'ocir;' => "\u{229a}",
        'ocirc;' => "\u{f4}",
        'ocy;' => "\u{43e}",
        'odash;' => "\u{229d}",
        'odblac;' => "\u{151}",
        'odiv;' => "\u{2a38}",
        'odot;' => "\u{2299}",
        'odsold;' => "\u{29bc}",
        'oelig;' => "\u{153}",
        'ofcir;' => "\u{29bf}",
        'ofr;' => "\u{1d52c}",
        'ogon;' => "\u{2db}",
        'ograve;' => "\u{f2}",
        'ogt;' => "\u{29c1}",
        'ohbar;' => "\u{29b5}",
        'ohm;' => "\u{3a9}",
        'oint;' => "\u{222e}",
        'olarr;' => "\u{21ba}",
        'olcir;' => "\u{29be}",
        'olcross;' => "\u{29bb}",
        'oline;' => "\u{203e}",
        'olt;' => "\u{29c0}",
        'omacr;' => "\u{14d}",
        'omega;' => "\u{3c9}",
        'omicron;' => "\u{3bf}",
        'omid;' => "\u{29b6}",
        'ominus;' => "\u{2296}",
        'oopf;' => "\u{1d560}",
        'opar;' => "\u{29b7}",
        'operp;' => "\u{29b9}",
        'oplus;' => "\u{2295}",
        'or;' => "\u{2228}",
        'orarr;' => "\u{21bb}",
        'ord;' => "\u{2a5d}",
        'order;' => "\u{2134}",
        'orderof;' => "\u{2134}",
        'ordf;' => "\u{aa}",
        'ordm;' => "\u{ba}",
        'origof;' => "\u{22b6}",
        'oror;' => "\u{2a56}",
        'orslope;' => "\u{2a57}",
        'orv;' => "\u{2a5b}",
        'oscr;' => "\u{2134}",
        'oslash;' => "\u{f8}",
        'osol;' => "\u{2298}",
        'otilde;' => "\u{f5}",
        'otimes;' => "\u{2297}",
        'otimesas;' => "\u{2a36}",
        'ouml;' => "\u{f6}",
        'ovbar;' => "\u{233d}",
        'par;' => "\u{2225}",
        'para;' => "\u{b6}",
        'parallel;' => "\u{2225}",
        'parsim;' => "\u{2af3}",
        'parsl;' => "\u{2afd}",
        'part;' => "\u{2202}",
        'pcy;' => "\u{43f}",
        'percnt;' => "\u{25}",
        'period;' => "\u{2e}",
        'permil;' => "\u{2030}",
        'perp;' => "\u{22a5}",
        'pertenk;' => "\u{2031}",
        'pfr;' => "\u{1d52d}",
        'phi;' => "\u{3c6}",
        'phiv;' => "\u{3d5}",
        'phmmat;' => "\u{2133}",
        'phone;' => "\u{260e}",
        'pi;' => "\u{3c0}",
        'pitchfork;' => "\u{22d4}",
        'piv;' => "\u{3d6}",
        'planck;' => "\u{210f}",
        'planckh;' => "\u{210e}",
        'plankv;' => "\u{210f}",
        'plus;' => "\u{2b}",
        'plusacir;' => "\u{2a23}",
        'plusb;' => "\u{229e}",
        'pluscir;' => "\u{2a22}",
        'plusdo;' => "\u{2214}",
        'plusdu;' => "\u{2a25}",
        'pluse;' => "\u{2a72}",
        'plusmn;' => "\u{b1}",
        'plussim;' => "\u{2a26}",
        'plustwo;' => "\u{2a27}",
        'pm;' => "\u{b1}",
        'pointint;' => "\u{2a15}",
        'popf;' => "\u{1d561}",
        'pound;' => "\u{a3}",
        'pr;' => "\u{227a}",
        'prE;' => "\u{2ab3}",
        'prap;' => "\u{2ab7}",
        'prcue;' => "\u{227c}",
        'pre;' => "\u{2aaf}",
        'prec;' => "\u{227a}",
        'precapprox;' => "\u{2ab7}",
        'preccurlyeq;' => "\u{227c}",
        'preceq;' => "\u{2aaf}",
        'precnapprox;' => "\u{2ab9}",
        'precneqq;' => "\u{2ab5}",
        'precnsim;' => "\u{22e8}",
        'precsim;' => "\u{227e}",
        'prime;' => "\u{2032}",
        'primes;' => "\u{2119}",
        'prnE;' => "\u{2ab5}",
        'prnap;' => "\u{2ab9}",
        'prnsim;' => "\u{22e8}",
        'prod;' => "\u{220f}",
        'profalar;' => "\u{232e}",
        'profline;' => "\u{2312}",
        'profsurf;' => "\u{2313}",
        'prop;' => "\u{221d}",
        'propto;' => "\u{221d}",
        'prsim;' => "\u{227e}",
        'prurel;' => "\u{22b0}",
        'pscr;' => "\u{1d4c5}",
        'psi;' => "\u{3c8}",
        'puncsp;' => "\u{2008}",
        'qfr;' => "\u{1d52e}",
        'qint;' => "\u{2a0c}",
        'qopf;' => "\u{1d562}",
        'qprime;' => "\u{2057}",
        'qscr;' => "\u{1d4c6}",
        'quaternions;' => "\u{210d}",
        'quatint;' => "\u{2a16}",
        'quest;' => "\u{3f}",
        'questeq;' => "\u{225f}",
        'quot;' => "\u{22}",
        'rAarr;' => "\u{21db}",
        'rArr;' => "\u{21d2}",
        'rAtail;' => "\u{291c}",
        'rBarr;' => "\u{290f}",
        'rHar;' => "\u{2964}",
        'race;' => "\u{223d}\u{331}",
        'racute;' => "\u{155}",
        'radic;' => "\u{221a}",
        'raemptyv;' => "\u{29b3}",
        'rang;' => "\u{27e9}",
        'rangd;' => "\u{2992}",
        'range;' => "\u{29a5}",
        'rangle;' => "\u{27e9}",
        'raquo;' => "\u{bb}",
        'rarr;' => "\u{2192}",
        'rarrap;' => "\u{2975}",
        'rarrb;' => "\u{21e5}",
        'rarrbfs;' => "\u{2920}",
        'rarrc;' => "\u{2933}",
        'rarrfs;' => "\u{291e}",
        'rarrhk;' => "\u{21aa}",
        'rarrlp;' => "\u{21ac}",
        'rarrpl;' => "\u{2945}",
        'rarrsim;' => "\u{2974}",
        'rarrtl;' => "\u{21a3}",
        'rarrw;' => "\u{219d}",
        'ratail;' => "\u{291a}",
        'ratio;' => "\u{2236}",
        'rationals;' => "\u{211a}",
        'rbarr;' => "\u{290d}",
        'rbbrk;' => "\u{2773}",
        'rbrace;' => "\u{7d}",
        'rbrack;' => "\u{5d}",
        'rbrke;' => "\u{298c}",
        'rbrksld;' => "\u{298e}",
        'rbrkslu;' => "\u{2990}",
        'rcaron;' => "\u{159}",
        'rcedil;' => "\u{157}",
        'rceil;' => "\u{2309}",
        'rcub;' => "\u{7d}",
        'rcy;' => "\u{440}",
        'rdca;' => "\u{2937}",
        'rdldhar;' => "\u{2969}",
        'rdquo;' => "\u{201d}",
        'rdquor;' => "\u{201d}",
        'rdsh;' => "\u{21b3}",
        'real;' => "\u{211c}",
        'realine;' => "\u{211b}",
        'realpart;' => "\u{211c}",
        'reals;' => "\u{211d}",
        'rect;' => "\u{25ad}",
        'reg;' => "\u{ae}",
        'rfisht;' => "\u{297d}",
        'rfloor;' => "\u{230b}",
        'rfr;' => "\u{1d52f}",
        'rhard;' => "\u{21c1}",
        'rharu;' => "\u{21c0}",
        'rharul;' => "\u{296c}",
        'rho;' => "\u{3c1}",
        'rhov;' => "\u{3f1}",
        'rightarrow;' => "\u{2192}",
        'rightarrowtail;' => "\u{21a3}",
        'rightharpoondown;' => "\u{21c1}",
        'rightharpoonup;' => "\u{21c0}",
        'rightleftarrows;' => "\u{21c4}",
        'rightleftharpoons;' => "\u{21cc}",
        'rightrightarrows;' => "\u{21c9}",
        'rightsquigarrow;' => "\u{219d}",
        'rightthreetimes;' => "\u{22cc}",
        'ring;' => "\u{2da}",
        'risingdotseq;' => "\u{2253}",
        'rlarr;' => "\u{21c4}",
        'rlhar;' => "\u{21cc}",
        'rlm;' => "\u{200f}",
        'rmoust;' => "\u{23b1}",
        'rmoustache;' => "\u{23b1}",
        'rnmid;' => "\u{2aee}",
        'roang;' => "\u{27ed}",
        'roarr;' => "\u{21fe}",
        'robrk;' => "\u{27e7}",
        'ropar;' => "\u{2986}",
        'ropf;' => "\u{1d563}",
        'roplus;' => "\u{2a2e}",
        'rotimes;' => "\u{2a35}",
        'rpar;' => "\u{29}",
        'rpargt;' => "\u{2994}",
        'rppolint;' => "\u{2a12}",
        'rrarr;' => "\u{21c9}",
        'rsaquo;' => "\u{203a}",
        'rscr;' => "\u{1d4c7}",
        'rsh;' => "\u{21b1}",
        'rsqb;' => "\u{5d}",
        'rsquo;' => "\u{2019}",
        'rsquor;' => "\u{2019}",
        'rthree;' => "\u{22cc}",
        'rtimes;' => "\u{22ca}",
        'rtri;' => "\u{25b9}",
        'rtrie;' => "\u{22b5}",
        'rtrif;' => "\u{25b8}",
        'rtriltri;' => "\u{29ce}",
        'ruluhar;' => "\u{2968}",
        'rx;' => "\u{211e}",
        'sacute;' => "\u{15b}",
        'sbquo;' => "\u{201a}",
        'sc;' => "\u{227b}",
        'scE;' => "\u{2ab4}",
        'scap;' => "\u{2ab8}",
        'scaron;' => "\u{161}",
        'sccue;' => "\u{227d}",
        'sce;' => "\u{2ab0}",
        'scedil;' => "\u{15f}",
        'scirc;' => "\u{15d}",
        'scnE;' => "\u{2ab6}",
        'scnap;' => "\u{2aba}",
        'scnsim;' => "\u{22e9}",
        'scpolint;' => "\u{2a13}",
        'scsim;' => "\u{227f}",
        'scy;' => "\u{441}",
        'sdot;' => "\u{22c5}",
        'sdotb;' => "\u{22a1}",
        'sdote;' => "\u{2a66}",
        'seArr;' => "\u{21d8}",
        'searhk;' => "\u{2925}",
        'searr;' => "\u{2198}",
        'searrow;' => "\u{2198}",
        'sect;' => "\u{a7}",
        'semi;' => "\u{3b}",
        'seswar;' => "\u{2929}",
        'setminus;' => "\u{2216}",
        'setmn;' => "\u{2216}",
        'sext;' => "\u{2736}",
        'sfr;' => "\u{1d530}",
        'sfrown;' => "\u{2322}",
        'sharp;' => "\u{266f}",
        'shchcy;' => "\u{449}",
        'shcy;' => "\u{448}",
        'shortmid;' => "\u{2223}",
        'shortparallel;' => "\u{2225}",
        'shy;' => "\u{ad}",
        'sigma;' => "\u{3c3}",
        'sigmaf;' => "\u{3c2}",
        'sigmav;' => "\u{3c2}",
        'sim;' => "\u{223c}",
        'simdot;' => "\u{2a6a}",
        'sime;' => "\u{2243}",
        'simeq;' => "\u{2243}",
        'simg;' => "\u{2a9e}",
        'simgE;' => "\u{2aa0}",
        'siml;' => "\u{2a9d}",
        'simlE;' => "\u{2a9f}",
        'simne;' => "\u{2246}",
        'simplus;' => "\u{2a24}",
        'simrarr;' => "\u{2972}",
        'slarr;' => "\u{2190}",
        'smallsetminus;' => "\u{2216}",
        'smashp;' => "\u{2a33}",
        'smeparsl;' => "\u{29e4}",
        'smid;' => "\u{2223}",
        'smile;' => "\u{2323}",
        'smt;' => "\u{2aaa}",
        'smte;' => "\u{2aac}",
        'smtes;' => "\u{2aac}\u{fe00}",
        'softcy;' => "\u{44c}",
        'sol;' => "\u{2f}",
        'solb;' => "\u{29c4}",
        'solbar;' => "\u{233f}",
        'sopf;' => "\u{1d564}",
        'spades;' => "\u{2660}",
        'spadesuit;' => "\u{2660}",
        'spar;' => "\u{2225}",
        'sqcap;' => "\u{2293}",
        'sqcaps;' => "\u{2293}\u{fe00}",
        'sqcup;' => "\u{2294}",
        'sqcups;' => "\u{2294}\u{fe00}",
        'sqsub;' => "\u{228f}",
        'sqsube;' => "\u{2291}",
        'sqsubset;' => "\u{228f}",
        'sqsubseteq;' => "\u{2291}",
        'sqsup;' => "\u{2290}",
        'sqsupe;' => "\u{2292}",
        'sqsupset;' => "\u{2290}",
        'sqsupseteq;' => "\u{2292}",
        'squ;' => "\u{25a1}",
        'square;' => "\u{25a1}",
        'squarf;' => "\u{25aa}",
        'squf;' => "\u{25aa}",
        'srarr;' => "\u{2192}",
        'sscr;' => "\u{1d4c8}",
        'ssetmn;' => "\u{2216}",
        'ssmile;' => "\u{2323}",
        'sstarf;' => "\u{22c6}",
        'star;' => "\u{2606}",
        'starf;' => "\u{2605}",
        'straightepsilon;' => "\u{3f5}",
        'straightphi;' => "\u{3d5}",
        'strns;' => "\u{af}",
        'sub;' => "\u{2282}",
        'subE;' => "\u{2ac5}",
        'subdot;' => "\u{2abd}",
        'sube;' => "\u{2286}",
        'subedot;' => "\u{2ac3}",
        'submult;' => "\u{2ac1}",
        'subnE;' => "\u{2acb}",
        'subne;' => "\u{228a}",
        'subplus;' => "\u{2abf}",
        'subrarr;' => "\u{2979}",
        'subset;' => "\u{2282}",
        'subseteq;' => "\u{2286}",
        'subseteqq;' => "\u{2ac5}",
        'subsetneq;' => "\u{228a}",
        'subsetneqq;' => "\u{2acb}",
        'subsim;' => "\u{2ac7}",
        'subsub;' => "\u{2ad5}",
        'subsup;' => "\u{2ad3}",
        'succ;' => "\u{227b}",
        'succapprox;' => "\u{2ab8}",
        'succcurlyeq;' => "\u{227d}",
        'succeq;' => "\u{2ab0}",
        'succnapprox;' => "\u{2aba}",
        'succneqq;' => "\u{2ab6}",
        'succnsim;' => "\u{22e9}",
        'succsim;' => "\u{227f}",
        'sum;' => "\u{2211}",
        'sung;' => "\u{266a}",
        'sup1;' => "\u{b9}",
        'sup2;' => "\u{b2}",
        'sup3;' => "\u{b3}",
        'sup;' => "\u{2283}",
        'supE;' => "\u{2ac6}",
        'supdot;' => "\u{2abe}",
        'supdsub;' => "\u{2ad8}",
        'supe;' => "\u{2287}",
        'supedot;' => "\u{2ac4}",
        'suphsol;' => "\u{27c9}",
        'suphsub;' => "\u{2ad7}",
        'suplarr;' => "\u{297b}",
        'supmult;' => "\u{2ac2}",
        'supnE;' => "\u{2acc}",
        'supne;' => "\u{228b}",
        'supplus;' => "\u{2ac0}",
        'supset;' => "\u{2283}",
        'supseteq;' => "\u{2287}",
        'supseteqq;' => "\u{2ac6}",
        'supsetneq;' => "\u{228b}",
        'supsetneqq;' => "\u{2acc}",
        'supsim;' => "\u{2ac8}",
        'supsub;' => "\u{2ad4}",
        'supsup;' => "\u{2ad6}",
        'swArr;' => "\u{21d9}",
        'swarhk;' => "\u{2926}",
        'swarr;' => "\u{2199}",
        'swarrow;' => "\u{2199}",
        'swnwar;' => "\u{292a}",
        'szlig;' => "\u{df}",
        'target;' => "\u{2316}",
        'tau;' => "\u{3c4}",
        'tbrk;' => "\u{23b4}",
        'tcaron;' => "\u{165}",
        'tcedil;' => "\u{163}",
        'tcy;' => "\u{442}",
        'tdot;' => "\u{20db}",
        'telrec;' => "\u{2315}",
        'tfr;' => "\u{1d531}",
        'there4;' => "\u{2234}",
        'therefore;' => "\u{2234}",
        'theta;' => "\u{3b8}",
        'thetasym;' => "\u{3d1}",
        'thetav;' => "\u{3d1}",
        'thickapprox;' => "\u{2248}",
        'thicksim;' => "\u{223c}",
        'thinsp;' => "\u{2009}",
        'thkap;' => "\u{2248}",
        'thksim;' => "\u{223c}",
        'thorn;' => "\u{fe}",
        'tilde;' => "\u{2dc}",
        'times;' => "\u{d7}",
        'timesb;' => "\u{22a0}",
        'timesbar;' => "\u{2a31}",
        'timesd;' => "\u{2a30}",
        'tint;' => "\u{222d}",
        'toea;' => "\u{2928}",
        'top;' => "\u{22a4}",
        'topbot;' => "\u{2336}",
        'topcir;' => "\u{2af1}",
        'topf;' => "\u{1d565}",
        'topfork;' => "\u{2ada}",
        'tosa;' => "\u{2929}",
        'tprime;' => "\u{2034}",
        'trade;' => "\u{2122}",
        'triangle;' => "\u{25b5}",
        'triangledown;' => "\u{25bf}",
        'triangleleft;' => "\u{25c3}",
        'trianglelefteq;' => "\u{22b4}",
        'triangleq;' => "\u{225c}",
        'triangleright;' => "\u{25b9}",
        'trianglerighteq;' => "\u{22b5}",
        'tridot;' => "\u{25ec}",
        'trie;' => "\u{225c}",
        'triminus;' => "\u{2a3a}",
        'triplus;' => "\u{2a39}",
        'trisb;' => "\u{29cd}",
        'tritime;' => "\u{2a3b}",
        'trpezium;' => "\u{23e2}",
        'tscr;' => "\u{1d4c9}",
        'tscy;' => "\u{446}",
        'tshcy;' => "\u{45b}",
        'tstrok;' => "\u{167}",
        'twixt;' => "\u{226c}",
        'twoheadleftarrow;' => "\u{219e}",
        'twoheadrightarrow;' => "\u{21a0}",
        'uArr;' => "\u{21d1}",
        'uHar;' => "\u{2963}",
        'uacute;' => "\u{fa}",
        'uarr;' => "\u{2191}",
        'ubrcy;' => "\u{45e}",
        'ubreve;' => "\u{16d}",
        'ucirc;' => "\u{fb}",
        'ucy;' => "\u{443}",
        'udarr;' => "\u{21c5}",
        'udblac;' => "\u{171}",
        'udhar;' => "\u{296e}",
        'ufisht;' => "\u{297e}",
        'ufr;' => "\u{1d532}",
        'ugrave;' => "\u{f9}",
        'uharl;' => "\u{21bf}",
        'uharr;' => "\u{21be}",
        'uhblk;' => "\u{2580}",
        'ulcorn;' => "\u{231c}",
        'ulcorner;' => "\u{231c}",
        'ulcrop;' => "\u{230f}",
        'ultri;' => "\u{25f8}",
        'umacr;' => "\u{16b}",
        'uml;' => "\u{a8}",
        'uogon;' => "\u{173}",
        'uopf;' => "\u{1d566}",
        'uparrow;' => "\u{2191}",
        'updownarrow;' => "\u{2195}",
        'upharpoonleft;' => "\u{21bf}",
        'upharpoonright;' => "\u{21be}",
        'uplus;' => "\u{228e}",
        'upsi;' => "\u{3c5}",
        'upsih;' => "\u{3d2}",
        'upsilon;' => "\u{3c5}",
        'upuparrows;' => "\u{21c8}",
        'urcorn;' => "\u{231d}",
        'urcorner;' => "\u{231d}",
        'urcrop;' => "\u{230e}",
        'uring;' => "\u{16f}",
        'urtri;' => "\u{25f9}",
        'uscr;' => "\u{1d4ca}",
        'utdot;' => "\u{22f0}",
        'utilde;' => "\u{169}",
        'utri;' => "\u{25b5}",
        'utrif;' => "\u{25b4}",
        'uuarr;' => "\u{21c8}",
        'uuml;' => "\u{fc}",
        'uwangle;' => "\u{29a7}",
        'vArr;' => "\u{21d5}",
        'vBar;' => "\u{2ae8}",
        'vBarv;' => "\u{2ae9}",
        'vDash;' => "\u{22a8}",
        'vangrt;' => "\u{299c}",
        'varepsilon;' => "\u{3f5}",
        'varkappa;' => "\u{3f0}",
        'varnothing;' => "\u{2205}",
        'varphi;' => "\u{3d5}",
        'varpi;' => "\u{3d6}",
        'varpropto;' => "\u{221d}",
        'varr;' => "\u{2195}",
        'varrho;' => "\u{3f1}",
        'varsigma;' => "\u{3c2}",
        'varsubsetneq;' => "\u{228a}\u{fe00}",
        'varsubsetneqq;' => "\u{2acb}\u{fe00}",
        'varsupsetneq;' => "\u{228b}\u{fe00}",
        'varsupsetneqq;' => "\u{2acc}\u{fe00}",
        'vartheta;' => "\u{3d1}",
        'vartriangleleft;' => "\u{22b2}",
        'vartriangleright;' => "\u{22b3}",
        'vcy;' => "\u{432}",
        'vdash;' => "\u{22a2}",
        'vee;' => "\u{2228}",
        'veebar;' => "\u{22bb}",
        'veeeq;' => "\u{225a}",
        'vellip;' => "\u{22ee}",
        'verbar;' => "\u{7c}",
        'vert;' => "\u{7c}",
        'vfr;' => "\u{1d533}",
        'vltri;' => "\u{22b2}",
        'vnsub;' => "\u{2282}\u{20d2}",
        'vnsup;' => "\u{2283}\u{20d2}",
        'vopf;' => "\u{1d567}",
        'vprop;' => "\u{221d}",
        'vrtri;' => "\u{22b3}",
        'vscr;' => "\u{1d4cb}",
        'vsubnE;' => "\u{2acb}\u{fe00}",
        'vsubne;' => "\u{228a}\u{fe00}",
        'vsupnE;' => "\u{2acc}\u{fe00}",
        'vsupne;' => "\u{228b}\u{fe00}",
        'vzigzag;' => "\u{299a}",
        'wcirc;' => "\u{175}",
        'wedbar;' => "\u{2a5f}",
        'wedge;' => "\u{2227}",
        'wedgeq;' => "\u{2259}",
        'weierp;' => "\u{2118}",
        'wfr;' => "\u{1d534}",
        'wopf;' => "\u{1d568}",
        'wp;' => "\u{2118}",
        'wr;' => "\u{2240}",
        'wreath;' => "\u{2240}",
        'wscr;' => "\u{1d4cc}",
        'xcap;' => "\u{22c2}",
        'xcirc;' => "\u{25ef}",
        'xcup;' => "\u{22c3}",
        'xdtri;' => "\u{25bd}",
        'xfr;' => "\u{1d535}",
        'xhArr;' => "\u{27fa}",
        'xharr;' => "\u{27f7}",
        'xi;' => "\u{3be}",
        'xlArr;' => "\u{27f8}",
        'xlarr;' => "\u{27f5}",
        'xmap;' => "\u{27fc}",
        'xnis;' => "\u{22fb}",
        'xodot;' => "\u{2a00}",
        'xopf;' => "\u{1d569}",
        'xoplus;' => "\u{2a01}",
        'xotime;' => "\u{2a02}",
        'xrArr;' => "\u{27f9}",
        'xrarr;' => "\u{27f6}",
        'xscr;' => "\u{1d4cd}",
        'xsqcup;' => "\u{2a06}",
        'xuplus;' => "\u{2a04}",
        'xutri;' => "\u{25b3}",
        'xvee;' => "\u{22c1}",
        'xwedge;' => "\u{22c0}",
        'yacute;' => "\u{fd}",
        'yacy;' => "\u{44f}",
        'ycirc;' => "\u{177}",
        'ycy;' => "\u{44b}",
        'yen;' => "\u{a5}",
        'yfr;' => "\u{1d536}",
        'yicy;' => "\u{457}",
        'yopf;' => "\u{1d56a}",
        'yscr;' => "\u{1d4ce}",
        'yucy;' => "\u{44e}",
        'yuml;' => "\u{ff}",
        'zacute;' => "\u{17a}",
        'zcaron;' => "\u{17e}",
        'zcy;' => "\u{437}",
        'zdot;' => "\u{17c}",
        'zeetrf;' => "\u{2128}",
        'zeta;' => "\u{3b6}",
        'zfr;' => "\u{1d537}",
        'zhcy;' => "\u{436}",
        'zigrarr;' => "\u{21dd}",
        'zopf;' => "\u{1d56b}",
        'zscr;' => "\u{1d4cf}",
        'zwj;' => "\u{200d}",
        'zwnj;' => "\u{200c}",
    ];
}

function __elephc_dom_legacy_entities(): array
{
    return ['AElig', 'AMP', 'Aacute', 'Acirc', 'Agrave', 'Aring', 'Atilde', 'Auml', 'COPY', 'Ccedil', 'ETH', 'Eacute', 'Ecirc', 'Egrave', 'Euml', 'GT', 'Iacute', 'Icirc', 'Igrave', 'Iuml', 'LT', 'Ntilde', 'Oacute', 'Ocirc', 'Ograve', 'Oslash', 'Otilde', 'Ouml', 'QUOT', 'REG', 'THORN', 'Uacute', 'Ucirc', 'Ugrave', 'Uuml', 'Yacute', 'aacute', 'acirc', 'acute', 'aelig', 'agrave', 'amp', 'aring', 'atilde', 'auml', 'brvbar', 'ccedil', 'cedil', 'cent', 'copy', 'curren', 'deg', 'divide', 'eacute', 'ecirc', 'egrave', 'eth', 'euml', 'frac12', 'frac14', 'frac34', 'gt', 'iacute', 'icirc', 'iexcl', 'igrave', 'iquest', 'iuml', 'laquo', 'lt', 'macr', 'micro', 'middot', 'nbsp', 'not', 'ntilde', 'oacute', 'ocirc', 'ograve', 'ordf', 'ordm', 'oslash', 'otilde', 'ouml', 'para', 'plusmn', 'pound', 'quot', 'raquo', 'reg', 'sect', 'shy', 'sup1', 'sup2', 'sup3', 'szlig', 'thorn', 'times', 'uacute', 'ucirc', 'ugrave', 'uml', 'uuml', 'yacute', 'yen', 'yuml'];
}
"####;
