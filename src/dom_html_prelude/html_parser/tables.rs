//! Purpose:
//! The libxml2 2.9.13 HTML4 tables (entities, auto-close pairs, void and boolean elements,
//! end-tag priorities) as `__ElephcHtmlTables`.
//!
//! Called from:
//! - `super::contribute`, which registers the class under `Part::Html`.
//!
//! Key details:
//! - Transcribed from the PHP model with `synthetic_class::transcribe_split`; the model was
//!   checked against `php -n` (see the module comment of `super`).

#![allow(clippy::all)]

use crate::parser::ast::*;
use crate::synthetic_class::*;

/// `__ElephcHtmlTables` — transcribed from the PHP form.
fn decl_class_elephchtmltables() -> Stmt {
    class("__ElephcHtmlTables")
        .final_()
        .method(
            method("entityByName")
                .static_()
                .returns(t_array())
                .body(vec![
                    s_assign("map", e_array(vec![])),
                    s_foreach(e_call("explode", vec![e_str(","), e_str("quot:34,amp:38,lt:60,gt:62,nbsp:160,iexcl:161,cent:162,pound:163,curren:164,yen:165,brvbar:166,sect:167,uml:168,copy:169,ordf:170,laquo:171,not:172,shy:173,reg:174,macr:175,deg:176,plusmn:177,sup2:178,sup3:179,acute:180,micro:181,para:182,middot:183,cedil:184,sup1:185,ordm:186,raquo:187,frac14:188,frac12:189,frac34:190,iquest:191,Agrave:192,Aacute:193,Acirc:194,Atilde:195,Auml:196,Aring:197,AElig:198,Ccedil:199,Egrave:200,Eacute:201,Ecirc:202,Euml:203,Igrave:204,Iacute:205,Icirc:206,Iuml:207,ETH:208,Ntilde:209,Ograve:210,Oacute:211,Ocirc:212,Otilde:213,Ouml:214,times:215,Oslash:216,Ugrave:217,Uacute:218,Ucirc:219,Uuml:220,Yacute:221,THORN:222,szlig:223,agrave:224,aacute:225,acirc:226,atilde:227,auml:228,aring:229,aelig:230,ccedil:231,egrave:232,eacute:233,ecirc:234,euml:235,igrave:236,iacute:237,icirc:238,iuml:239,eth:240,ntilde:241,ograve:242,oacute:243,ocirc:244,otilde:245,ouml:246,divide:247,oslash:248,ugrave:249,uacute:250,ucirc:251,uuml:252,yacute:253,thorn:254,yuml:255,OElig:338,oelig:339,Scaron:352,scaron:353,Yuml:376,fnof:402,circ:710,tilde:732,Alpha:913,Beta:914,Gamma:915,Delta:916,Epsilon:917,Zeta:918,Eta:919,Theta:920,Iota:921,Kappa:922,Lambda:923,Mu:924,Nu:925,Xi:926,Omicron:927,Pi:928,Rho:929,Sigma:931,Tau:932,Upsilon:933,Phi:934,Chi:935,Psi:936,Omega:937,alpha:945,beta:946,gamma:947,delta:948,epsilon:949,zeta:950,eta:951,theta:952,iota:953,kappa:954,lambda:955,mu:956,nu:957,xi:958,omicron:959,pi:960,rho:961,sigmaf:962,sigma:963,tau:964,upsilon:965,phi:966,chi:967,psi:968,omega:969,thetasym:977,upsih:978,piv:982,ensp:8194,emsp:8195,thinsp:8201,zwnj:8204,zwj:8205,lrm:8206,rlm:8207,ndash:8211,mdash:8212,lsquo:8216,rsquo:8217,sbquo:8218,ldquo:8220,rdquo:8221,bdquo:8222,dagger:8224,Dagger:8225,bull:8226,hellip:8230,permil:8240,prime:8242,Prime:8243,lsaquo:8249,rsaquo:8250,oline:8254,frasl:8260,euro:8364,image:8465,weierp:8472,real:8476,trade:8482,alefsym:8501,larr:8592,uarr:8593,rarr:8594,darr:8595,harr:8596,crarr:8629,lArr:8656,uArr:8657,rArr:8658,dArr:8659,hArr:8660,forall:8704,part:8706,exist:8707,empty:8709,nabla:8711,isin:8712,notin:8713,ni:8715,prod:8719,sum:8721,minus:8722,lowast:8727,radic:8730,prop:8733,infin:8734,ang:8736,and:8743,or:8744,cap:8745,cup:8746,int:8747,there4:8756,sim:8764,cong:8773,asymp:8776,ne:8800,equiv:8801,le:8804,ge:8805,sub:8834,sup:8835,nsub:8836,sube:8838,supe:8839,oplus:8853,otimes:8855,perp:8869,sdot:8901,lceil:8968,rceil:8969,lfloor:8970,rfloor:8971,lang:9001,rang:9002,loz:9674,spades:9824,clubs:9827,hearts:9829,diams:9830,apos:39")]), None, "pair", vec![
                        s_assign("parts", e_call("explode", vec![e_str(":"), e_var("pair")])),
                        s_array_assign("map", e_index(e_var("parts"), e_int(0)), e_cast(CastType::Int, e_index(e_var("parts"), e_int(1)))),
                    ]),
                    s_return(e_var("map")),
                ]),
        )
        .method(
            method("entityByValue")
                .static_()
                .returns(t_array())
                .body(vec![
                    s_assign("map", e_array(vec![])),
                    s_foreach(e_call("explode", vec![e_str(","), e_str("quot:34,amp:38,lt:60,gt:62,nbsp:160,iexcl:161,cent:162,pound:163,curren:164,yen:165,brvbar:166,sect:167,uml:168,copy:169,ordf:170,laquo:171,not:172,shy:173,reg:174,macr:175,deg:176,plusmn:177,sup2:178,sup3:179,acute:180,micro:181,para:182,middot:183,cedil:184,sup1:185,ordm:186,raquo:187,frac14:188,frac12:189,frac34:190,iquest:191,Agrave:192,Aacute:193,Acirc:194,Atilde:195,Auml:196,Aring:197,AElig:198,Ccedil:199,Egrave:200,Eacute:201,Ecirc:202,Euml:203,Igrave:204,Iacute:205,Icirc:206,Iuml:207,ETH:208,Ntilde:209,Ograve:210,Oacute:211,Ocirc:212,Otilde:213,Ouml:214,times:215,Oslash:216,Ugrave:217,Uacute:218,Ucirc:219,Uuml:220,Yacute:221,THORN:222,szlig:223,agrave:224,aacute:225,acirc:226,atilde:227,auml:228,aring:229,aelig:230,ccedil:231,egrave:232,eacute:233,ecirc:234,euml:235,igrave:236,iacute:237,icirc:238,iuml:239,eth:240,ntilde:241,ograve:242,oacute:243,ocirc:244,otilde:245,ouml:246,divide:247,oslash:248,ugrave:249,uacute:250,ucirc:251,uuml:252,yacute:253,thorn:254,yuml:255,OElig:338,oelig:339,Scaron:352,scaron:353,Yuml:376,fnof:402,circ:710,tilde:732,Alpha:913,Beta:914,Gamma:915,Delta:916,Epsilon:917,Zeta:918,Eta:919,Theta:920,Iota:921,Kappa:922,Lambda:923,Mu:924,Nu:925,Xi:926,Omicron:927,Pi:928,Rho:929,Sigma:931,Tau:932,Upsilon:933,Phi:934,Chi:935,Psi:936,Omega:937,alpha:945,beta:946,gamma:947,delta:948,epsilon:949,zeta:950,eta:951,theta:952,iota:953,kappa:954,lambda:955,mu:956,nu:957,xi:958,omicron:959,pi:960,rho:961,sigmaf:962,sigma:963,tau:964,upsilon:965,phi:966,chi:967,psi:968,omega:969,thetasym:977,upsih:978,piv:982,ensp:8194,emsp:8195,thinsp:8201,zwnj:8204,zwj:8205,lrm:8206,rlm:8207,ndash:8211,mdash:8212,lsquo:8216,rsquo:8217,sbquo:8218,ldquo:8220,rdquo:8221,bdquo:8222,dagger:8224,Dagger:8225,bull:8226,hellip:8230,permil:8240,prime:8242,Prime:8243,lsaquo:8249,rsaquo:8250,oline:8254,frasl:8260,euro:8364,image:8465,weierp:8472,real:8476,trade:8482,alefsym:8501,larr:8592,uarr:8593,rarr:8594,darr:8595,harr:8596,crarr:8629,lArr:8656,uArr:8657,rArr:8658,dArr:8659,hArr:8660,forall:8704,part:8706,exist:8707,empty:8709,nabla:8711,isin:8712,notin:8713,ni:8715,prod:8719,sum:8721,minus:8722,lowast:8727,radic:8730,prop:8733,infin:8734,ang:8736,and:8743,or:8744,cap:8745,cup:8746,int:8747,there4:8756,sim:8764,cong:8773,asymp:8776,ne:8800,equiv:8801,le:8804,ge:8805,sub:8834,sup:8835,nsub:8836,sube:8838,supe:8839,oplus:8853,otimes:8855,perp:8869,sdot:8901,lceil:8968,rceil:8969,lfloor:8970,rfloor:8971,lang:9001,rang:9002,loz:9674,spades:9824,clubs:9827,hearts:9829,diams:9830,apos:39")]), None, "pair", vec![
                        s_assign("parts", e_call("explode", vec![e_str(":"), e_var("pair")])),
                        s_array_assign("map", e_cast(CastType::Int, e_index(e_var("parts"), e_int(1))), e_index(e_var("parts"), e_int(0))),
                    ]),
                    s_return(e_var("map")),
                ]),
        )
        .method(
            method("closeMap")
                .static_()
                .returns(t_array())
                .body(vec![
                    s_assign("map", e_array(vec![])),
                    s_foreach(e_call("explode", vec![e_str(" "), e_str("a>a a>fieldset a>table a>td a>th address>dd address>dl address>dt address>form address>li address>ul b>center b>p b>td b>th big>p caption>col caption>colgroup caption>tbody caption>tfoot caption>thead caption>tr colgroup>colgroup colgroup>tbody colgroup>tfoot colgroup>thead colgroup>tr dd>dt dir>dd dir>dl dir>dt dir>form dir>ul dl>form dl>li dt>dd dt>dl font>center font>td font>th form>form h1>fieldset h1>form h1>li h1>p h1>table h2>fieldset h2>form h2>li h2>p h2>table h3>fieldset h3>form h3>li h3>p h3>table h4>fieldset h4>form h4>li h4>p h4>table h5>fieldset h5>form h5>li h5>p h5>table h6>fieldset h6>form h6>li h6>p h6>table head>a head>abbr head>acronym head>address head>b head>bdo head>big head>blockquote head>body head>br head>center head>cite head>code head>dd head>dfn head>dir head>div head>dl head>dt head>em head>fieldset head>font head>form head>frameset head>h1 head>h2 head>h3 head>h4 head>h5 head>h6 head>hr head>i head>iframe head>img head>kbd head>li head>map head>menu head>ol head>p head>pre head>q head>s head>samp head>small head>span head>strike head>strong head>sub head>sup head>table head>tt head>u head>ul head>var i>center i>p i>td i>th legend>fieldset li>li menu>dd menu>dl menu>dt menu>form menu>ul ol>form ol>ul option>optgroup option>option p>address p>blockquote p>body p>caption p>center p>col p>colgroup p>dd p>dir p>div p>dl p>dt p>fieldset p>form p>frameset p>h1 p>h2 p>h3 p>h4 p>h5 p>h6 p>head p>hr p>li p>menu p>ol p>p p>pre p>table p>tbody p>td p>tfoot p>th p>title p>tr p>ul pre>dd pre>dl pre>dt pre>fieldset pre>form pre>li pre>table pre>ul s>p small>p span>td span>th strike>p tbody>tbody tbody>tfoot td>tbody td>td td>tfoot td>th td>tr tfoot>tbody th>tbody th>td th>tfoot th>th th>tr thead>tbody thead>tfoot title>body title>frameset tr>tbody tr>tfoot tr>tr tt>p u>p u>td u>th ul>address ul>form ul>menu ul>ol ul>pre script>noscript style>body style>frameset")]), None, "pair", vec![
                        s_array_assign("map", e_var("pair"), e_bool(true)),
                    ]),
                    s_return(e_var("map")),
                ]),
        )
        .method(
            method("words")
                .static_()
                .param("list", TypeExpr::Str)
                .returns(t_array())
                .body(vec![
                    s_assign("map", e_array(vec![])),
                    s_foreach(e_call("explode", vec![e_str(" "), e_var("list")]), None, "word", vec![
                        s_array_assign("map", e_var("word"), e_bool(true)),
                    ]),
                    s_return(e_var("map")),
                ]),
        )
        .method(
            method("voidTags")
                .static_()
                .returns(t_array())
                .body(vec![
                    s_return(e_self_call("words", vec![e_str("area base basefont br col frame hr img input isindex link meta param")])),
                ]),
        )
        .method(
            method("knownTags")
                .static_()
                .returns(t_array())
                .body(vec![
                    s_return(e_self_call("words", vec![e_str("a abbr acronym address applet area b base basefont bdo big blockquote body br button caption center cite code col colgroup dd del dfn dir div dl dt em embed fieldset font form frame frameset h1 h2 h3 h4 h5 h6 head hr html i iframe img input ins isindex kbd label legend li link map menu meta noframes noscript object ol optgroup option p param pre q s samp script select small span strike strong style sub sup table tbody td textarea tfoot th thead title tr tt u ul var")])),
                ]),
        )
        .method(
            method("blockTags")
                .static_()
                .returns(t_array())
                .body(vec![
                    s_return(e_self_call("words", vec![e_str("address area base blockquote body caption center col colgroup dd dir div dl dt fieldset form frame frameset h1 h2 h3 h4 h5 h6 head hr html isindex legend li link menu meta noframes noscript ol optgroup option p param pre style table tbody td tfoot th thead title tr ul")])),
                ]),
        )
        .method(
            method("booleanAttrs")
                .static_()
                .returns(t_array())
                .body(vec![
                    s_return(e_self_call("words", vec![e_str("checked compact declare defer disabled ismap multiple nohref noresize noshade nowrap readonly selected")])),
                ]),
        )
        .method(
            method("allowPcdata")
                .static_()
                .returns(t_array())
                .body(vec![
                    s_return(e_self_call("words", vec![e_str("a abbr acronym address applet b bdo big blockquote body button caption center cite code dd del dfn div dt em font form h1 h2 h3 h4 h5 h6 i iframe ins kbd label legend li noscript object p pre q s samp small span strike strong td th tt u var")])),
                ]),
        )
        .method(
            method("endPriority")
                .static_()
                .param("name", TypeExpr::Str)
                .returns(TypeExpr::Int)
                .body(vec![
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("div")),
                        vec![
                            s_return(e_int(150)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_var("name"), BinOp::StrictEq, e_str("td")), BinOp::Or, e_binop(e_var("name"), BinOp::StrictEq, e_str("th"))),
                        vec![
                            s_return(e_int(160)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("tr")),
                        vec![
                            s_return(e_int(170)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_binop(e_var("name"), BinOp::StrictEq, e_str("thead")), BinOp::Or, e_binop(e_var("name"), BinOp::StrictEq, e_str("tbody"))), BinOp::Or, e_binop(e_var("name"), BinOp::StrictEq, e_str("tfoot"))),
                        vec![
                            s_return(e_int(180)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("table")),
                        vec![
                            s_return(e_int(220)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("head")),
                        vec![
                            s_return(e_int(230)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("body")),
                        vec![
                            s_return(e_int(240)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("html")),
                        vec![
                            s_return(e_int(250)),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_int(100)),
                ]),
        )
        .build()
}


/// The tables class declaration.
pub(super) fn declaration() -> Stmt {
    decl_class_elephchtmltables()
}
