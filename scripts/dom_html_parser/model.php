<?php
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
    public array $kind = [];
    public array $nm = [];
    public array $vl = [];
    public array $at = [];
    public array $nx = [];
    public array $pv = [];
    public array $fc = [];
    public array $lc = [];
    public int $dtdId = -1;
    public int $latin1End = 0;
    public string $s = '';
    public int $pos = 0;
    public int $len = 0;
    public int $options = 0;
    public array $names = [];
    public array $stk = [];
    public int $nodeTop = 0;
    public int $depth = 0;
    public int $seen = 0;
    public array $closeMap = [];
    public array $voidTags = [];
    public array $entities = [];
    public array $booleanAttrs = [];
    public array $allowPcdata = [];
    public bool $keepBlanks = true;

    public function __construct(string $source, int $options)
    {
        $this->newNode(0, '#document', '');
        $this->options = $options;
        $this->keepBlanks = ($options & 256) === 0;
        $this->s = self::decodeSource($source);
        if (self::hasHighByte($source) && !str_starts_with($source, '<?xml')) {
            $cut = self::metaUtf8End($source);
            $this->latin1End = $cut < 0 ? strlen($this->s) : strlen(self::latin1ToUtf8(substr($source, 0, $cut)));
        }
        $this->len = strlen($this->s);
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

    public function topId(): int
    {
        // The stack is a slot array plus a depth, so a pop never frees an element.
        return $this->nodeTop === 0 ? -1 : $this->stk[$this->nodeTop - 1];
    }

    public function newNode(int $kind, string $name, string $value): int
    {
        $id = count($this->kind);
        $this->kind[] = $kind;
        $this->nm[] = $name;
        $this->vl[] = $value;
        $this->at[] = [];
        $this->nx[] = -1;
        $this->pv[] = -1;
        $this->fc[] = -1;
        $this->lc[] = -1;
        return $id;
    }

    public function appendTo(int $parent, int $id): void
    {
        $last = $this->lc[$parent];
        $this->pv[$id] = $last;
        if ($last < 0) {
            $this->fc[$parent] = $id;
        } else {
            $this->nx[$last] = $id;
        }
        $this->lc[$parent] = $id;
    }

    public function prependToDocument(int $id): void
    {
        $first = $this->fc[0];
        $this->nx[$id] = $first;
        if ($first < 0) {
            $this->lc[0] = $id;
        } else {
            $this->pv[$first] = $id;
        }
        $this->fc[0] = $id;
    }

    public function parse(): bool
    {
        $this->closeMap = __ElephcHtmlTables::closeMap();
        $this->voidTags = __ElephcHtmlTables::voidTags();
        $this->entities = __ElephcHtmlTables::entityByName();
        $this->booleanAttrs = __ElephcHtmlTables::booleanAttrs();
        $this->allowPcdata = __ElephcHtmlTables::allowPcdata();
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
        if (($this->options & 4) === 0 && $this->dtdId < 0) {
            $dtd = $this->newNode(6, 'html', '-//W3C//DTD HTML 4.0 Transitional//EN');
            $this->at[$dtd] = ['http://www.w3.org/TR/REC-html40/loose.dtd'];
            $this->dtdId = $dtd;
            $this->prependToDocument($dtd);
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
        $element = $this->newNode(1, $local, '');
        $pairs = [];
        foreach ($attrs as $attrName => $value) {
            if (isset($valueless[(string) $attrName])) {
                $pairs[] = [(string) $attrName, (string) $value, true];
            } else {
                $pairs[] = [(string) $attrName, (string) $value];
            }
        }
        $this->at[$element] = $pairs;
        $parent = $this->topId();
        if ($this->fc[0] < 0) {
            $this->appendTo(0, $element);
        } else {
            if ($parent < 0) {
                $parent = $this->fc[0];
            }
            if ($this->kind[$parent] === 1) {
                $this->appendTo($parent, $element);
            } else {
                $this->appendTo(0, $element);
            }
        }
        if (count($this->stk) > $this->nodeTop) {
            $this->stk[$this->nodeTop] = $element;
        } else {
            $this->stk[] = $element;
        }
        $this->nodeTop++;
    }

    public function saxEndElement(): void
    {
        $this->nodeTop--;
    }

    public function saxText(int $kind, string $name, string $text): void
    {
        $node = $this->topId();
        if ($node < 0 || $text === '') {
            return;
        }
        $last = $this->lc[$node];
        if ($last >= 0 && $this->kind[$last] === $kind) {
            $this->vl[$last] = $this->vl[$last] . $text;
            return;
        }
        $this->appendTo($node, $this->newNode($kind, $name, $text));
    }

    public function saxCharacters(string $text): void
    {
        $this->saxText(2, '#text', $text);
    }

    public function saxCdata(string $text): void
    {
        $this->saxText(3, '#cdata-section', $text);
    }

    public function saxAddMisc(int $id): void
    {
        $parent = $this->topId();
        if ($parent < 0 || $this->kind[$parent] !== 1) {
            $this->appendTo(0, $id);
        } else {
            $this->appendTo($parent, $id);
        }
    }

    // Folds the flat node table into the tree arrays `__elephc_dom_make_children` reads.
    public function tree(int $id): mixed
    {
        $children = [];
        $child = $this->fc[$id];
        while ($child >= 0) {
            $children[] = $this->tree($child);
            $child = $this->nx[$child];
        }
        $kind = $this->kind[$id];
        $label = 'element';
        if ($kind === 0) {
            $label = 'document';
        } elseif ($kind === 2) {
            $label = 'text';
        } elseif ($kind === 3) {
            $label = 'cdata';
        } elseif ($kind === 4) {
            $label = 'comment';
        } elseif ($kind === 5) {
            $label = 'pi';
        } elseif ($kind === 6) {
            $label = 'doctype';
        }
        return __elephc_dom_new_node($label, $this->nm[$id], $this->vl[$id], $this->at[$id], $children);
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
            if ($c === '<' && $this->nxt(1) === '!' && $this->startsWithNoCaseAt(2, 'doctype')) {
                // libxml2 does not loop back after a misplaced DOCTYPE: the same pass goes on
                // with whatever follows it, so a tag right after it is parsed as a start tag.
                $this->parseDoctype();
                $c = $this->cur();
            }
            if ($c === '') {
                continue;
            }
            if ($c === '<' && $this->nxt(1) === '!') {
                if ($this->nxt(2) === '-' && $this->nxt(3) === '-') {
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
        $comment = $this->newNode(4, '#comment', substr($this->s, $this->pos + 4, $end - $this->pos - 4));
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
        $name = substr($this->s, $start, $this->pos - $start);
        if ($start < $this->latin1End && self::hasHighByte($name)) {
            // libxml2's Latin-1 name path loses the ASCII prefix before the first high byte, or the
            // high character itself when the name starts with one.
            $first = 0;
            while (ord($name[$first]) < 128) {
                $first++;
            }
            $name = substr($name, $first === 0 ? 2 : $first);
        }
        return $name;
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
            $this->saxAddMisc($this->newNode(5, $target, ''));
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
        $this->saxAddMisc($this->newNode(5, $target, $data));
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
        if ($name === null || $this->dtdId >= 0) {
            return;
        }
        $dtd = $this->newNode(6, $name, $public === null ? '' : $public);
        $this->at[$dtd] = [$system === null ? '' : $system];
        $this->dtdId = $dtd;
        $this->prependToDocument($dtd);
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
        if ($name === 'body' && $this->dtdId >= 0 && $this->vl[$this->dtdId] !== '') {
            $id = strtolower($this->vl[$this->dtdId]);
            if ($id === '-//w3c//dtd html 4.01//en' || $id === '-//w3c//dtd html 4//en') {
                return true;
            }
        }
        $node = $this->topId();
        if ($node < 0) {
            return false;
        }
        $last = $this->lc[$node];
        while ($last >= 0 && $this->kind[$last] === 4) {
            $last = $this->pv[$last];
        }
        if ($last < 0) {
            if (isset($this->allowPcdata[$name])) {
                return false;
            }
        } elseif ($this->kind[$last] === 2 || $this->kind[$last] === 3) {
            return false;
        } elseif (isset($this->allowPcdata[$this->nm[$last]])) {
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
            $this->saxCharacters(self::utf8($value));
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
        $this->saxCharacters(self::utf8($ref[1]));
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
                        $out .= self::utf8($value);
                    }
                } else {
                    $ref = $this->parseEntityRef();
                    if ($ref[0] === null) {
                        $out .= '&';
                    } elseif ($ref[1] === null) {
                        $out .= '&' . $ref[0];
                    } else {
                        $out .= self::utf8($ref[1]);
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
