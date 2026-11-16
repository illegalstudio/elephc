//! Purpose:
//! HTML entity decoding for the legacy libxml-compatible DOM prelude.
//!
//! Called from:
//! - `crate::dom_html_prelude::declarations()` extends the built surface with these helpers.
//!
//! Key details:
//! - The case-sensitive table contains HTML4 named entities plus XML apos.
//! - Numeric references use XML character ranges and bounded Unicode accumulation.
//! - Decoding is a single pass, so an escaped ampersand cannot decode twice.
//! - Byte probes return scalars to avoid retaining cloned string-return arguments.

use crate::parser::ast::*;
use crate::synthetic_class::*;

/// `__elephc_dom_named_entity_codepoint` — transcribed from the PHP form.
fn decl_fn_elephc_dom_named_entity_codepoint() -> Stmt {
    function("__elephc_dom_named_entity_codepoint")
        .param("name", TypeExpr::Str)
        .returns(TypeExpr::Int)
        .body(vec![
            s_static("_entities", e_array_assoc(vec![(e_str("AElig"), e_int(198)), (e_str("Aacute"), e_int(193)), (e_str("Acirc"), e_int(194)), (e_str("Agrave"), e_int(192)), (e_str("Alpha"), e_int(913)), (e_str("Aring"), e_int(197)), (e_str("Atilde"), e_int(195)), (e_str("Auml"), e_int(196)), (e_str("Beta"), e_int(914)), (e_str("Ccedil"), e_int(199)), (e_str("Chi"), e_int(935)), (e_str("Dagger"), e_int(8225)), (e_str("Delta"), e_int(916)), (e_str("ETH"), e_int(208)), (e_str("Eacute"), e_int(201)), (e_str("Ecirc"), e_int(202)), (e_str("Egrave"), e_int(200)), (e_str("Epsilon"), e_int(917)), (e_str("Eta"), e_int(919)), (e_str("Euml"), e_int(203)), (e_str("Gamma"), e_int(915)), (e_str("Iacute"), e_int(205)), (e_str("Icirc"), e_int(206)), (e_str("Igrave"), e_int(204)), (e_str("Iota"), e_int(921)), (e_str("Iuml"), e_int(207)), (e_str("Kappa"), e_int(922)), (e_str("Lambda"), e_int(923)), (e_str("Mu"), e_int(924)), (e_str("Ntilde"), e_int(209)), (e_str("Nu"), e_int(925)), (e_str("OElig"), e_int(338)), (e_str("Oacute"), e_int(211)), (e_str("Ocirc"), e_int(212)), (e_str("Ograve"), e_int(210)), (e_str("Omega"), e_int(937)), (e_str("Omicron"), e_int(927)), (e_str("Oslash"), e_int(216)), (e_str("Otilde"), e_int(213)), (e_str("Ouml"), e_int(214)), (e_str("Phi"), e_int(934)), (e_str("Pi"), e_int(928)), (e_str("Prime"), e_int(8243)), (e_str("Psi"), e_int(936)), (e_str("Rho"), e_int(929)), (e_str("Scaron"), e_int(352)), (e_str("Sigma"), e_int(931)), (e_str("THORN"), e_int(222)), (e_str("Tau"), e_int(932)), (e_str("Theta"), e_int(920)), (e_str("Uacute"), e_int(218)), (e_str("Ucirc"), e_int(219)), (e_str("Ugrave"), e_int(217)), (e_str("Upsilon"), e_int(933)), (e_str("Uuml"), e_int(220)), (e_str("Xi"), e_int(926)), (e_str("Yacute"), e_int(221)), (e_str("Yuml"), e_int(376)), (e_str("Zeta"), e_int(918)), (e_str("aacute"), e_int(225)), (e_str("acirc"), e_int(226)), (e_str("acute"), e_int(180)), (e_str("aelig"), e_int(230)), (e_str("agrave"), e_int(224)), (e_str("alefsym"), e_int(8501)), (e_str("alpha"), e_int(945)), (e_str("amp"), e_int(38)), (e_str("and"), e_int(8743)), (e_str("ang"), e_int(8736)), (e_str("apos"), e_int(39)), (e_str("aring"), e_int(229)), (e_str("asymp"), e_int(8776)), (e_str("atilde"), e_int(227)), (e_str("auml"), e_int(228)), (e_str("bdquo"), e_int(8222)), (e_str("beta"), e_int(946)), (e_str("brvbar"), e_int(166)), (e_str("bull"), e_int(8226)), (e_str("cap"), e_int(8745)), (e_str("ccedil"), e_int(231)), (e_str("cedil"), e_int(184)), (e_str("cent"), e_int(162)), (e_str("chi"), e_int(967)), (e_str("circ"), e_int(710)), (e_str("clubs"), e_int(9827)), (e_str("cong"), e_int(8773)), (e_str("copy"), e_int(169)), (e_str("crarr"), e_int(8629)), (e_str("cup"), e_int(8746)), (e_str("curren"), e_int(164)), (e_str("dArr"), e_int(8659)), (e_str("dagger"), e_int(8224)), (e_str("darr"), e_int(8595)), (e_str("deg"), e_int(176)), (e_str("delta"), e_int(948)), (e_str("diams"), e_int(9830)), (e_str("divide"), e_int(247)), (e_str("eacute"), e_int(233)), (e_str("ecirc"), e_int(234)), (e_str("egrave"), e_int(232)), (e_str("empty"), e_int(8709)), (e_str("emsp"), e_int(8195)), (e_str("ensp"), e_int(8194)), (e_str("epsilon"), e_int(949)), (e_str("equiv"), e_int(8801)), (e_str("eta"), e_int(951)), (e_str("eth"), e_int(240)), (e_str("euml"), e_int(235)), (e_str("euro"), e_int(8364)), (e_str("exist"), e_int(8707)), (e_str("fnof"), e_int(402)), (e_str("forall"), e_int(8704)), (e_str("frac12"), e_int(189)), (e_str("frac14"), e_int(188)), (e_str("frac34"), e_int(190)), (e_str("frasl"), e_int(8260)), (e_str("gamma"), e_int(947)), (e_str("ge"), e_int(8805)), (e_str("gt"), e_int(62)), (e_str("hArr"), e_int(8660)), (e_str("harr"), e_int(8596)), (e_str("hearts"), e_int(9829)), (e_str("hellip"), e_int(8230)), (e_str("iacute"), e_int(237)), (e_str("icirc"), e_int(238)), (e_str("iexcl"), e_int(161)), (e_str("igrave"), e_int(236)), (e_str("image"), e_int(8465)), (e_str("infin"), e_int(8734)), (e_str("int"), e_int(8747)), (e_str("iota"), e_int(953)), (e_str("iquest"), e_int(191)), (e_str("isin"), e_int(8712)), (e_str("iuml"), e_int(239)), (e_str("kappa"), e_int(954)), (e_str("lArr"), e_int(8656)), (e_str("lambda"), e_int(955)), (e_str("lang"), e_int(9001)), (e_str("laquo"), e_int(171)), (e_str("larr"), e_int(8592)), (e_str("lceil"), e_int(8968)), (e_str("ldquo"), e_int(8220)), (e_str("le"), e_int(8804)), (e_str("lfloor"), e_int(8970)), (e_str("lowast"), e_int(8727)), (e_str("loz"), e_int(9674)), (e_str("lrm"), e_int(8206)), (e_str("lsaquo"), e_int(8249)), (e_str("lsquo"), e_int(8216)), (e_str("lt"), e_int(60)), (e_str("macr"), e_int(175)), (e_str("mdash"), e_int(8212)), (e_str("micro"), e_int(181)), (e_str("middot"), e_int(183)), (e_str("minus"), e_int(8722)), (e_str("mu"), e_int(956)), (e_str("nabla"), e_int(8711)), (e_str("nbsp"), e_int(160)), (e_str("ndash"), e_int(8211)), (e_str("ne"), e_int(8800)), (e_str("ni"), e_int(8715)), (e_str("not"), e_int(172)), (e_str("notin"), e_int(8713)), (e_str("nsub"), e_int(8836)), (e_str("ntilde"), e_int(241)), (e_str("nu"), e_int(957)), (e_str("oacute"), e_int(243)), (e_str("ocirc"), e_int(244)), (e_str("oelig"), e_int(339)), (e_str("ograve"), e_int(242)), (e_str("oline"), e_int(8254)), (e_str("omega"), e_int(969)), (e_str("omicron"), e_int(959)), (e_str("oplus"), e_int(8853)), (e_str("or"), e_int(8744)), (e_str("ordf"), e_int(170)), (e_str("ordm"), e_int(186)), (e_str("oslash"), e_int(248)), (e_str("otilde"), e_int(245)), (e_str("otimes"), e_int(8855)), (e_str("ouml"), e_int(246)), (e_str("para"), e_int(182)), (e_str("part"), e_int(8706)), (e_str("permil"), e_int(8240)), (e_str("perp"), e_int(8869)), (e_str("phi"), e_int(966)), (e_str("pi"), e_int(960)), (e_str("piv"), e_int(982)), (e_str("plusmn"), e_int(177)), (e_str("pound"), e_int(163)), (e_str("prime"), e_int(8242)), (e_str("prod"), e_int(8719)), (e_str("prop"), e_int(8733)), (e_str("psi"), e_int(968)), (e_str("quot"), e_int(34)), (e_str("rArr"), e_int(8658)), (e_str("radic"), e_int(8730)), (e_str("rang"), e_int(9002)), (e_str("raquo"), e_int(187)), (e_str("rarr"), e_int(8594)), (e_str("rceil"), e_int(8969)), (e_str("rdquo"), e_int(8221)), (e_str("real"), e_int(8476)), (e_str("reg"), e_int(174)), (e_str("rfloor"), e_int(8971)), (e_str("rho"), e_int(961)), (e_str("rlm"), e_int(8207)), (e_str("rsaquo"), e_int(8250)), (e_str("rsquo"), e_int(8217)), (e_str("sbquo"), e_int(8218)), (e_str("scaron"), e_int(353)), (e_str("sdot"), e_int(8901)), (e_str("sect"), e_int(167)), (e_str("shy"), e_int(173)), (e_str("sigma"), e_int(963)), (e_str("sigmaf"), e_int(962)), (e_str("sim"), e_int(8764)), (e_str("spades"), e_int(9824)), (e_str("sub"), e_int(8834)), (e_str("sube"), e_int(8838)), (e_str("sum"), e_int(8721)), (e_str("sup"), e_int(8835)), (e_str("sup1"), e_int(185)), (e_str("sup2"), e_int(178)), (e_str("sup3"), e_int(179)), (e_str("supe"), e_int(8839)), (e_str("szlig"), e_int(223)), (e_str("tau"), e_int(964)), (e_str("there4"), e_int(8756)), (e_str("theta"), e_int(952)), (e_str("thetasym"), e_int(977)), (e_str("thinsp"), e_int(8201)), (e_str("thorn"), e_int(254)), (e_str("tilde"), e_int(732)), (e_str("times"), e_int(215)), (e_str("trade"), e_int(8482)), (e_str("uArr"), e_int(8657)), (e_str("uacute"), e_int(250)), (e_str("uarr"), e_int(8593)), (e_str("ucirc"), e_int(251)), (e_str("ugrave"), e_int(249)), (e_str("uml"), e_int(168)), (e_str("upsih"), e_int(978)), (e_str("upsilon"), e_int(965)), (e_str("uuml"), e_int(252)), (e_str("weierp"), e_int(8472)), (e_str("xi"), e_int(958)), (e_str("yacute"), e_int(253)), (e_str("yen"), e_int(165)), (e_str("yuml"), e_int(255)), (e_str("zeta"), e_int(950)), (e_str("zwj"), e_int(8205)), (e_str("zwnj"), e_int(8204))])),
            s_if(
                e_call("isset", vec![e_index(e_var("_entities"), e_var("name"))]),
                vec![
                    s_return(e_index(e_var("_entities"), e_var("name"))),
                ],
                vec![],
                None,
            ),
            s_return(e_int(0)),
        ])
        .build()
}

/// `__elephc_dom_entity_utf8` — transcribed from the PHP form.
fn decl_fn_elephc_dom_entity_utf8() -> Stmt {
    function("__elephc_dom_entity_utf8")
        .param("code", TypeExpr::Int)
        .returns(TypeExpr::Str)
        .body(vec![
            s_if(
                e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_var("code"), BinOp::Lt, e_int(0)), BinOp::Or, e_binop(e_var("code"), BinOp::Gt, e_int(1114111))), BinOp::Or, e_binop(e_binop(e_binop(e_binop(e_var("code"), BinOp::Lt, e_int(32)), BinOp::And, e_binop(e_var("code"), BinOp::StrictNotEq, e_int(9))), BinOp::And, e_binop(e_var("code"), BinOp::StrictNotEq, e_int(10))), BinOp::And, e_binop(e_var("code"), BinOp::StrictNotEq, e_int(13)))), BinOp::Or, e_binop(e_binop(e_var("code"), BinOp::GtEq, e_int(55296)), BinOp::And, e_binop(e_var("code"), BinOp::LtEq, e_int(57343)))), BinOp::Or, e_binop(e_var("code"), BinOp::StrictEq, e_int(65534))), BinOp::Or, e_binop(e_var("code"), BinOp::StrictEq, e_int(65535))),
                vec![
                    s_return(e_str("")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("code"), BinOp::Lt, e_int(128)),
                vec![
                    s_return(e_call("chr", vec![e_var("code")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("code"), BinOp::Lt, e_int(2048)),
                vec![
                    s_return(e_binop(e_call("chr", vec![e_binop(e_int(192), BinOp::BitOr, e_binop(e_var("code"), BinOp::ShiftRight, e_int(6)))]), BinOp::Concat, e_call("chr", vec![e_binop(e_int(128), BinOp::BitOr, e_binop(e_var("code"), BinOp::BitAnd, e_int(63)))]))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("code"), BinOp::Lt, e_int(65536)),
                vec![
                    s_return(e_binop(e_binop(e_call("chr", vec![e_binop(e_int(224), BinOp::BitOr, e_binop(e_var("code"), BinOp::ShiftRight, e_int(12)))]), BinOp::Concat, e_call("chr", vec![e_binop(e_int(128), BinOp::BitOr, e_binop(e_binop(e_var("code"), BinOp::ShiftRight, e_int(6)), BinOp::BitAnd, e_int(63)))])), BinOp::Concat, e_call("chr", vec![e_binop(e_int(128), BinOp::BitOr, e_binop(e_var("code"), BinOp::BitAnd, e_int(63)))]))),
                ],
                vec![],
                None,
            ),
            s_return(e_binop(e_binop(e_binop(e_call("chr", vec![e_binop(e_int(240), BinOp::BitOr, e_binop(e_var("code"), BinOp::ShiftRight, e_int(18)))]), BinOp::Concat, e_call("chr", vec![e_binop(e_int(128), BinOp::BitOr, e_binop(e_binop(e_var("code"), BinOp::ShiftRight, e_int(12)), BinOp::BitAnd, e_int(63)))])), BinOp::Concat, e_call("chr", vec![e_binop(e_int(128), BinOp::BitOr, e_binop(e_binop(e_var("code"), BinOp::ShiftRight, e_int(6)), BinOp::BitAnd, e_int(63)))])), BinOp::Concat, e_call("chr", vec![e_binop(e_int(128), BinOp::BitOr, e_binop(e_var("code"), BinOp::BitAnd, e_int(63)))]))),
        ])
        .build()
}

/// `__elephc_dom_entity_byte` — transcribed from the PHP form.
fn decl_fn_elephc_dom_entity_byte() -> Stmt {
    function("__elephc_dom_entity_byte")
        .param("text", TypeExpr::Str)
        .param("index", TypeExpr::Int)
        .returns(TypeExpr::Int)
        .body(vec![
            s_if(
                e_binop(e_binop(e_var("index"), BinOp::Lt, e_int(0)), BinOp::Or, e_binop(e_var("index"), BinOp::GtEq, e_call("strlen", vec![e_var("text")]))),
                vec![
                    s_return(e_neg(e_int(1))),
                ],
                vec![],
                None,
            ),
            s_assign("_char", e_index(e_var("text"), e_var("index"))),
            s_return(e_call("ord", vec![e_var("_char")])),
        ])
        .build()
}

/// `__elephc_dom_entity_digit` — transcribed from the PHP form.
fn decl_fn_elephc_dom_entity_digit() -> Stmt {
    function("__elephc_dom_entity_digit")
        .param("_ord", TypeExpr::Int)
        .param("base", TypeExpr::Int)
        .returns(TypeExpr::Int)
        .body(vec![
            s_if(
                e_binop(e_binop(e_var("_ord"), BinOp::GtEq, e_int(48)), BinOp::And, e_binop(e_var("_ord"), BinOp::LtEq, e_int(57))),
                vec![
                    s_return(e_binop(e_var("_ord"), BinOp::Sub, e_int(48))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_binop(e_var("base"), BinOp::StrictEq, e_int(16)), BinOp::And, e_binop(e_var("_ord"), BinOp::GtEq, e_int(65))), BinOp::And, e_binop(e_var("_ord"), BinOp::LtEq, e_int(70))),
                vec![
                    s_return(e_binop(e_var("_ord"), BinOp::Sub, e_int(55))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_binop(e_var("base"), BinOp::StrictEq, e_int(16)), BinOp::And, e_binop(e_var("_ord"), BinOp::GtEq, e_int(97))), BinOp::And, e_binop(e_var("_ord"), BinOp::LtEq, e_int(102))),
                vec![
                    s_return(e_binop(e_var("_ord"), BinOp::Sub, e_int(87))),
                ],
                vec![],
                None,
            ),
            s_return(e_neg(e_int(1))),
        ])
        .build()
}

/// `__elephc_dom_decode_entities` — transcribed from the PHP form.
fn decl_fn_elephc_dom_decode_entities() -> Stmt {
    function("__elephc_dom_decode_entities")
        .param("text", TypeExpr::Str)
        .returns(TypeExpr::Str)
        .body(vec![
            s_if(
                e_binop(e_call("strpos", vec![e_var("text"), e_str("&")]), BinOp::StrictEq, e_bool(false)),
                vec![
                    s_return(e_var("text")),
                ],
                vec![],
                None,
            ),
            s_assign("_out", e_str("")),
            s_assign("_i", e_int(0)),
            s_assign("_len", e_call("strlen", vec![e_var("text")])),
            s_while(e_binop(e_var("_i"), BinOp::Lt, e_var("_len")), vec![
                s_assign("_current", e_call("__elephc_dom_entity_byte", vec![e_var("text"), e_var("_i")])),
                s_if(
                    e_binop(e_var("_current"), BinOp::StrictNotEq, e_int(38)),
                    vec![
                        s_assign("_start", e_var("_i")),
                        s_while(e_binop(e_var("_i"), BinOp::Lt, e_var("_len")), vec![
                            s_assign("_current", e_call("__elephc_dom_entity_byte", vec![e_var("text"), e_var("_i")])),
                            s_if(
                                e_binop(e_var("_current"), BinOp::StrictEq, e_int(38)),
                                vec![
                                    s_break(1),
                                ],
                                vec![],
                                None,
                            ),
                            s_expr(e_post_inc("_i")),
                        ]),
                        s_assign("_chunk", e_call("substr", vec![e_var("text"), e_var("_start"), e_binop(e_var("_i"), BinOp::Sub, e_var("_start"))])),
                        s_assign("_out", e_binop(e_var("_out"), BinOp::Concat, e_var("_chunk"))),
                        s_continue(1),
                    ],
                    vec![],
                    None,
                ),
                s_assign("_next", e_call("__elephc_dom_entity_byte", vec![e_var("text"), e_binop(e_var("_i"), BinOp::Add, e_int(1))])),
                s_if(
                    e_binop(e_var("_next"), BinOp::StrictEq, e_int(35)),
                    vec![
                        s_assign("_i", e_binop(e_var("_i"), BinOp::Add, e_int(2))),
                        s_assign("_base", e_int(10)),
                        s_assign("_char", e_call("__elephc_dom_entity_byte", vec![e_var("text"), e_var("_i")])),
                        s_if(
                            e_binop(e_binop(e_var("_char"), BinOp::StrictEq, e_int(120)), BinOp::Or, e_binop(e_var("_char"), BinOp::StrictEq, e_int(88))),
                            vec![
                                s_assign("_base", e_int(16)),
                                s_expr(e_post_inc("_i")),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("_code", e_int(0)),
                        s_while(e_binop(e_var("_i"), BinOp::Lt, e_var("_len")), vec![
                            s_assign("_current", e_call("__elephc_dom_entity_byte", vec![e_var("text"), e_var("_i")])),
                            s_assign("_digit", e_call("__elephc_dom_entity_digit", vec![e_var("_current"), e_var("_base")])),
                            s_if(
                                e_binop(e_binop(e_var("_digit"), BinOp::Lt, e_int(0)), BinOp::Or, e_binop(e_var("_digit"), BinOp::GtEq, e_var("_base"))),
                                vec![
                                    s_break(1),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_binop(e_var("_code"), BinOp::LtEq, e_int(1114111)),
                                vec![
                                    s_assign("_code", e_binop(e_binop(e_var("_code"), BinOp::Mul, e_var("_base")), BinOp::Add, e_var("_digit"))),
                                ],
                                vec![],
                                None,
                            ),
                            s_expr(e_post_inc("_i")),
                        ]),
                        s_assign("_current", e_call("__elephc_dom_entity_byte", vec![e_var("text"), e_var("_i")])),
                        s_if(
                            e_binop(e_var("_current"), BinOp::StrictEq, e_int(59)),
                            vec![
                                s_expr(e_post_inc("_i")),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("_decoded", e_call("__elephc_dom_entity_utf8", vec![e_var("_code")])),
                        s_assign("_out", e_binop(e_var("_out"), BinOp::Concat, e_var("_decoded"))),
                        s_continue(1),
                    ],
                    vec![],
                    None,
                ),
                s_assign("_end", e_binop(e_var("_i"), BinOp::Add, e_int(1))),
                s_while(e_binop(e_var("_end"), BinOp::Lt, e_var("_len")), vec![
                    s_assign("_current", e_call("__elephc_dom_entity_byte", vec![e_var("text"), e_var("_end")])),
                    s_if(
                        e_not(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_var("_current"), BinOp::GtEq, e_int(65)), BinOp::And, e_binop(e_var("_current"), BinOp::LtEq, e_int(90))), BinOp::Or, e_binop(e_binop(e_var("_current"), BinOp::GtEq, e_int(97)), BinOp::And, e_binop(e_var("_current"), BinOp::LtEq, e_int(122)))), BinOp::Or, e_binop(e_binop(e_var("_current"), BinOp::GtEq, e_int(48)), BinOp::And, e_binop(e_var("_current"), BinOp::LtEq, e_int(57)))), BinOp::Or, e_binop(e_var("_current"), BinOp::StrictEq, e_int(45))), BinOp::Or, e_binop(e_var("_current"), BinOp::StrictEq, e_int(95))), BinOp::Or, e_binop(e_var("_current"), BinOp::StrictEq, e_int(58)))),
                        vec![
                            s_break(1),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_post_inc("_end")),
                ]),
                s_assign("_current", e_call("__elephc_dom_entity_byte", vec![e_var("text"), e_var("_end")])),
                s_if(
                    e_binop(e_var("_current"), BinOp::StrictEq, e_int(59)),
                    vec![
                        s_assign("_name", e_call("substr", vec![e_var("text"), e_binop(e_var("_i"), BinOp::Add, e_int(1)), e_binop(e_binop(e_var("_end"), BinOp::Sub, e_var("_i")), BinOp::Sub, e_int(1))])),
                        s_assign("_code", e_call("__elephc_dom_named_entity_codepoint", vec![e_var("_name")])),
                        s_if(
                            e_binop(e_var("_code"), BinOp::StrictNotEq, e_int(0)),
                            vec![
                                s_assign("_decoded", e_call("__elephc_dom_entity_utf8", vec![e_var("_code")])),
                                s_assign("_out", e_binop(e_var("_out"), BinOp::Concat, e_var("_decoded"))),
                                s_assign("_i", e_binop(e_var("_end"), BinOp::Add, e_int(1))),
                                s_continue(1),
                            ],
                            vec![],
                            None,
                        ),
                    ],
                    vec![],
                    None,
                ),
                s_assign("_out", e_binop(e_var("_out"), BinOp::Concat, e_str("&"))),
                s_expr(e_post_inc("_i")),
            ]),
            s_return(e_var("_out")),
        ])
        .build()
}

/// Builds this module's declarations, one helper per declaration above.
pub(super) fn declarations() -> Program {
    internal_declarations(|| {
        vec![
            decl_fn_elephc_dom_named_entity_codepoint(),
            decl_fn_elephc_dom_entity_utf8(),
            decl_fn_elephc_dom_entity_byte(),
            decl_fn_elephc_dom_entity_digit(),
            decl_fn_elephc_dom_decode_entities(),
        ]
    })
}
