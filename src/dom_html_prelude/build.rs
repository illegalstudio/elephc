//! Purpose:
//! The `Dom\\` classes of PHP 8.4's DOM API for HTML documents, built as AST.
//!
//! Called from:
//! - `crate::dom_html_prelude::declarations`.
//!
//! Key details:
//! - TRANSCRIBED, not written: every declaration was generated from the parse of the PHP model
//!   in `fragments` (`synthetic_class::transcribe`), which was differential-tested against php's
//!   own `Dom\\HTMLDocument` -- see the module documentation of `dom_html_prelude`. Edit the PHP
//!   model and re-transcribe rather than editing these builders by hand; `oracle` compares the
//!   two node by node.
//! - One helper per declaration (and per large method): a prelude built as one nested builder
//!   expression overflows the stack.

#![allow(clippy::all)]

use crate::parser::ast::{BinOp, CastType, Program, Stmt, TypeExpr};
use crate::synthetic_class::*;

/// `Dom\\NodeList` — transcribed from the PHP form.
fn decl_class_dom_nodelist() -> Stmt {
    class("Dom\\NodeList")
        .implements("\\IteratorAggregate")
        .implements("\\Countable")
        .prop("length", TypeExpr::Int, Some(e_int(0)))
        .prop("items", t_array(), Some(e_array(vec![])))
        .method(decl_class_dom_nodelist_m1())
        .method(decl_class_dom_nodelist_m2())
        .method(decl_class_dom_nodelist_m3())
        .method(decl_class_dom_nodelist_m4())
        .build()
}

/// `Dom\\HTMLCollection` — transcribed from the PHP form.
fn decl_class_dom_htmlcollection() -> Stmt {
    class("Dom\\HTMLCollection")
        .implements("\\IteratorAggregate")
        .implements("\\Countable")
        .prop("length", TypeExpr::Int, Some(e_int(0)))
        .prop("items", t_array(), Some(e_array(vec![])))
        .method(decl_class_dom_htmlcollection_m5())
        .method(decl_class_dom_htmlcollection_m6())
        .method(decl_class_dom_htmlcollection_m7())
        .method(decl_class_dom_htmlcollection_m8())
        .method(decl_class_dom_htmlcollection_m9())
        .build()
}

/// `Dom\\NamedNodeMap` — transcribed from the PHP form.
fn decl_class_dom_namednodemap() -> Stmt {
    class("Dom\\NamedNodeMap")
        .implements("\\IteratorAggregate")
        .implements("\\Countable")
        .prop("length", TypeExpr::Int, Some(e_int(0)))
        .prop("items", t_array(), Some(e_array(vec![])))
        .method(decl_class_dom_namednodemap_m10())
        .method(decl_class_dom_namednodemap_m11())
        .method(decl_class_dom_namednodemap_m12())
        .method(decl_class_dom_namednodemap_m13())
        .method(decl_class_dom_namednodemap_m14())
        .build()
}

/// `Dom\\Node` — transcribed from the PHP form.
fn decl_class_dom_node() -> Stmt {
    class("Dom\\Node")
        .constant("ELEMENT_NODE", e_int(1))
        .constant("ATTRIBUTE_NODE", e_int(2))
        .constant("TEXT_NODE", e_int(3))
        .constant("CDATA_SECTION_NODE", e_int(4))
        .constant("ENTITY_REFERENCE_NODE", e_int(5))
        .constant("ENTITY_NODE", e_int(6))
        .constant("PROCESSING_INSTRUCTION_NODE", e_int(7))
        .constant("COMMENT_NODE", e_int(8))
        .constant("DOCUMENT_NODE", e_int(9))
        .constant("DOCUMENT_TYPE_NODE", e_int(10))
        .constant("DOCUMENT_FRAGMENT_NODE", e_int(11))
        .constant("NOTATION_NODE", e_int(12))
        .constant("HTML_DOCUMENT_NODE", e_int(13))
        .prop("nodeType", TypeExpr::Int, Some(e_int(0)))
        .prop("nodeName", TypeExpr::Str, Some(e_str("")))
        .prop("baseURI", TypeExpr::Str, Some(e_str("about:blank")))
        .prop("isConnected", TypeExpr::Bool, Some(e_bool(false)))
        .prop("ownerDocument", t_nullable(t_class("\\Dom\\Document")), Some(e_null()))
        .prop("parentNode", t_nullable(t_class("\\Dom\\Node")), Some(e_null()))
        .prop("parentElement", t_nullable(t_class("\\Dom\\Element")), Some(e_null()))
        .prop("childNodes", t_class("\\Dom\\NodeList"), None)
        .prop("firstChild", t_nullable(t_class("\\Dom\\Node")), Some(e_null()))
        .prop("lastChild", t_nullable(t_class("\\Dom\\Node")), Some(e_null()))
        .prop("previousSibling", t_nullable(t_class("\\Dom\\Node")), Some(e_null()))
        .prop("nextSibling", t_nullable(t_class("\\Dom\\Node")), Some(e_null()))
        .prop("nodeValue", t_nullable(TypeExpr::Str), Some(e_null()))
        .prop("textContent", t_nullable(TypeExpr::Str), Some(e_null()))
        .prop("kids", t_array(), Some(e_array(vec![])))
        .method(decl_class_dom_node_m15())
        .method(decl_class_dom_node_m16())
        .method(decl_class_dom_node_m17())
        .method(decl_class_dom_node_m18())
        .method(decl_class_dom_node_m19())
        .method(decl_class_dom_node_m20())
        .method(decl_class_dom_node_m21())
        .method(decl_class_dom_node_m22())
        .method(decl_class_dom_node_m23())
        .method(decl_class_dom_node_m24())
        .method(decl_class_dom_node_m25())
        .method(decl_class_dom_node_m26())
        .method(decl_class_dom_node_m27())
        .method(decl_class_dom_node_m28())
        .build()
}

/// `Dom\\CharacterData` — transcribed from the PHP form.
fn decl_class_dom_characterdata() -> Stmt {
    class("Dom\\CharacterData")
        .extends("\\Dom\\Node")
        .prop("data", TypeExpr::Str, Some(e_str("")))
        .prop("length", TypeExpr::Int, Some(e_int(0)))
        .prop("previousElementSibling", t_nullable(t_class("\\Dom\\Element")), Some(e_null()))
        .prop("nextElementSibling", t_nullable(t_class("\\Dom\\Element")), Some(e_null()))
        .method(decl_class_dom_characterdata_m29())
        .method(decl_class_dom_characterdata_m30())
        .build()
}

/// `Dom\\Text` — transcribed from the PHP form.
fn decl_class_dom_text() -> Stmt {
    class("Dom\\Text")
        .extends("\\Dom\\CharacterData")
        .prop("wholeText", TypeExpr::Str, Some(e_str("")))
        .method(decl_class_dom_text_m31())
        .build()
}

/// `Dom\\Comment` — transcribed from the PHP form.
fn decl_class_dom_comment() -> Stmt {
    class("Dom\\Comment")
        .extends("\\Dom\\CharacterData")
        .method(decl_class_dom_comment_m32())
        .build()
}

/// `Dom\\DocumentType` — transcribed from the PHP form.
fn decl_class_dom_documenttype() -> Stmt {
    class("Dom\\DocumentType")
        .extends("\\Dom\\Node")
        .prop("name", TypeExpr::Str, Some(e_str("")))
        .prop("publicId", TypeExpr::Str, Some(e_str("")))
        .prop("systemId", TypeExpr::Str, Some(e_str("")))
        .method(decl_class_dom_documenttype_m33())
        .build()
}

/// `Dom\\Attr` — transcribed from the PHP form.
fn decl_class_dom_attr() -> Stmt {
    class("Dom\\Attr")
        .extends("\\Dom\\Node")
        .prop("namespaceURI", t_nullable(TypeExpr::Str), Some(e_null()))
        .prop("prefix", t_nullable(TypeExpr::Str), Some(e_null()))
        .prop("localName", TypeExpr::Str, Some(e_str("")))
        .prop("name", TypeExpr::Str, Some(e_str("")))
        .prop("value", TypeExpr::Str, Some(e_str("")))
        .prop("ownerElement", t_nullable(t_class("\\Dom\\Element")), Some(e_null()))
        .prop("specified", TypeExpr::Bool, Some(e_bool(true)))
        .method(decl_class_dom_attr_m34())
        .method(decl_class_dom_attr_m35())
        .build()
}

/// `Dom\\Element` — transcribed from the PHP form.
fn decl_class_dom_element() -> Stmt {
    class("Dom\\Element")
        .extends("\\Dom\\Node")
        .prop("namespaceURI", t_nullable(TypeExpr::Str), Some(e_str("http://www.w3.org/1999/xhtml")))
        .prop("prefix", t_nullable(TypeExpr::Str), Some(e_null()))
        .prop("localName", TypeExpr::Str, Some(e_str("")))
        .prop("tagName", TypeExpr::Str, Some(e_str("")))
        .prop("id", TypeExpr::Str, Some(e_str("")))
        .prop("className", TypeExpr::Str, Some(e_str("")))
        .prop("attributes", t_class("\\Dom\\NamedNodeMap"), None)
        .prop("children", t_class("\\Dom\\HTMLCollection"), None)
        .prop("firstElementChild", t_nullable(t_class("\\Dom\\Element")), Some(e_null()))
        .prop("lastElementChild", t_nullable(t_class("\\Dom\\Element")), Some(e_null()))
        .prop("childElementCount", TypeExpr::Int, Some(e_int(0)))
        .prop("previousElementSibling", t_nullable(t_class("\\Dom\\Element")), Some(e_null()))
        .prop("nextElementSibling", t_nullable(t_class("\\Dom\\Element")), Some(e_null()))
        .method(decl_class_dom_element_m36())
        .method(decl_class_dom_element_m37())
        .method(decl_class_dom_element_m38())
        .method(decl_class_dom_element_m39())
        .method(decl_class_dom_element_m40())
        .method(decl_class_dom_element_m41())
        .method(decl_class_dom_element_m42())
        .method(decl_class_dom_element_m43())
        .method(decl_class_dom_element_m44())
        .method(decl_class_dom_element_m45())
        .method(decl_class_dom_element_m46())
        .method(decl_class_dom_element_m47())
        .build()
}

/// `Dom\\DocumentFragment` — transcribed from the PHP form.
fn decl_class_dom_documentfragment() -> Stmt {
    class("Dom\\DocumentFragment")
        .extends("\\Dom\\Node")
        .method(decl_class_dom_documentfragment_m48())
        .build()
}

/// `Dom\\Document` — transcribed from the PHP form.
fn decl_class_dom_document() -> Stmt {
    class("Dom\\Document")
        .extends("\\Dom\\Node")
        .prop("building", TypeExpr::Bool, Some(e_bool(false)))
        .prop("doctype", t_nullable(t_class("\\Dom\\DocumentType")), Some(e_null()))
        .prop("documentElement", t_nullable(t_class("\\Dom\\Element")), Some(e_null()))
        .prop("URL", TypeExpr::Str, Some(e_str("about:blank")))
        .prop("documentURI", TypeExpr::Str, Some(e_str("about:blank")))
        .prop("characterSet", TypeExpr::Str, Some(e_str("UTF-8")))
        .prop("charset", TypeExpr::Str, Some(e_str("UTF-8")))
        .prop("inputEncoding", TypeExpr::Str, Some(e_str("UTF-8")))
        .prop("contentType", TypeExpr::Str, Some(e_str("text/html")))
        .prop("body", t_nullable(t_class("\\Dom\\Element")), Some(e_null()))
        .prop("head", t_nullable(t_class("\\Dom\\Element")), Some(e_null()))
        .prop("title", TypeExpr::Str, Some(e_str("")))
        .method(decl_class_dom_document_m49())
        .method(decl_class_dom_document_m50())
        .method(decl_class_dom_document_m51())
        .method(decl_class_dom_document_m52())
        .method(decl_class_dom_document_m53())
        .method(decl_class_dom_document_m54())
        .method(decl_class_dom_document_m55())
        .method(decl_class_dom_document_m56())
        .build()
}

/// `Dom\\HTMLDocument` — transcribed from the PHP form.
fn decl_class_dom_htmldocument() -> Stmt {
    class("Dom\\HTMLDocument")
        .final_()
        .extends("\\Dom\\Document")
        .method(decl_class_dom_htmldocument_m57())
        .method(decl_class_dom_htmldocument_m58())
        .build()
}

/// `Dom\\__ElephcHtmlParser` — transcribed from the PHP form.
fn decl_class_dom_elephchtmlparser() -> Stmt {
    class("Dom\\__ElephcHtmlParser")
        .final_()
        .constant("SVG", e_str("http://www.w3.org/2000/svg"))
        .constant("MATHML", e_str("http://www.w3.org/1998/Math/MathML"))
        .constant("HTMLNS", e_str("http://www.w3.org/1999/xhtml"))
        .private_prop("doc", t_class("\\Dom\\Document"), None)
        .private_prop("src", TypeExpr::Str, Some(e_str("")))
        .private_prop("pos", TypeExpr::Int, Some(e_int(0)))
        .private_prop("len", TypeExpr::Int, Some(e_int(0)))
        .private_prop("entityTable", t_array(), Some(e_array(vec![])))
        .private_prop("legacyEntities", t_array(), Some(e_array(vec![])))
        .private_prop("entitiesLoaded", TypeExpr::Bool, Some(e_bool(false)))
        .private_prop("rawTag", TypeExpr::Str, Some(e_str("")))
        .private_prop("rawMode", TypeExpr::Str, Some(e_str("")))
        .private_prop("mode", TypeExpr::Str, Some(e_str("initial")))
        .private_prop("origMode", TypeExpr::Str, Some(e_str("")))
        .private_prop("open", t_array(), Some(e_array(vec![])))
        .private_prop("afe", t_array(), Some(e_array(vec![])))
        .private_prop("headEl", t_nullable(t_class("\\Dom\\Element")), Some(e_null()))
        .private_prop("formEl", t_nullable(t_class("\\Dom\\Element")), Some(e_null()))
        .private_prop("framesetOk", TypeExpr::Bool, Some(e_bool(true)))
        .private_prop("quirks", TypeExpr::Bool, Some(e_bool(true)))
        .private_prop("foster", TypeExpr::Bool, Some(e_bool(false)))
        .private_prop("stop", TypeExpr::Bool, Some(e_bool(false)))
        .private_prop("pendingText", TypeExpr::Str, Some(e_str("")))
        .private_prop("tableTexts", t_array(), Some(e_array(vec![])))
        .private_prop("templateModes", t_array(), Some(e_array(vec![])))
        .private_prop("unterminated", TypeExpr::Bool, Some(e_bool(false)))
        .private_prop("tableTextBuf", TypeExpr::Str, Some(e_str("")))
        .method(decl_class_dom_elephchtmlparser_m59())
        .method(decl_class_dom_elephchtmlparser_m60())
        .method(decl_class_dom_elephchtmlparser_m61())
        .method(decl_class_dom_elephchtmlparser_m62())
        .method(decl_class_dom_elephchtmlparser_m63())
        .method(decl_class_dom_elephchtmlparser_m64())
        .method(decl_class_dom_elephchtmlparser_m65())
        .method(decl_class_dom_elephchtmlparser_m66())
        .method(decl_class_dom_elephchtmlparser_m67())
        .method(decl_class_dom_elephchtmlparser_m68())
        .method(decl_class_dom_elephchtmlparser_m69())
        .method(decl_class_dom_elephchtmlparser_m70())
        .method(decl_class_dom_elephchtmlparser_m71())
        .method(decl_class_dom_elephchtmlparser_m72())
        .method(decl_class_dom_elephchtmlparser_m73())
        .method(decl_class_dom_elephchtmlparser_m74())
        .method(decl_class_dom_elephchtmlparser_m75())
        .method(decl_class_dom_elephchtmlparser_m76())
        .method(decl_class_dom_elephchtmlparser_m77())
        .method(decl_class_dom_elephchtmlparser_m78())
        .method(decl_class_dom_elephchtmlparser_m79())
        .method(decl_class_dom_elephchtmlparser_m80())
        .method(decl_class_dom_elephchtmlparser_m81())
        .method(decl_class_dom_elephchtmlparser_m82())
        .method(decl_class_dom_elephchtmlparser_m83())
        .method(decl_class_dom_elephchtmlparser_m84())
        .method(decl_class_dom_elephchtmlparser_m85())
        .method(decl_class_dom_elephchtmlparser_m86())
        .method(decl_class_dom_elephchtmlparser_m87())
        .method(decl_class_dom_elephchtmlparser_m88())
        .method(decl_class_dom_elephchtmlparser_m89())
        .method(decl_class_dom_elephchtmlparser_m90())
        .method(decl_class_dom_elephchtmlparser_m91())
        .method(decl_class_dom_elephchtmlparser_m92())
        .method(decl_class_dom_elephchtmlparser_m93())
        .method(decl_class_dom_elephchtmlparser_m94())
        .method(decl_class_dom_elephchtmlparser_m95())
        .method(decl_class_dom_elephchtmlparser_m96())
        .method(decl_class_dom_elephchtmlparser_m97())
        .method(decl_class_dom_elephchtmlparser_m98())
        .method(decl_class_dom_elephchtmlparser_m99())
        .method(decl_class_dom_elephchtmlparser_m100())
        .method(decl_class_dom_elephchtmlparser_m101())
        .method(decl_class_dom_elephchtmlparser_m102())
        .method(decl_class_dom_elephchtmlparser_m103())
        .method(decl_class_dom_elephchtmlparser_m104())
        .method(decl_class_dom_elephchtmlparser_m105())
        .method(decl_class_dom_elephchtmlparser_m106())
        .method(decl_class_dom_elephchtmlparser_m107())
        .method(decl_class_dom_elephchtmlparser_m108())
        .method(decl_class_dom_elephchtmlparser_m109())
        .method(decl_class_dom_elephchtmlparser_m110())
        .method(decl_class_dom_elephchtmlparser_m111())
        .method(decl_class_dom_elephchtmlparser_m112())
        .method(decl_class_dom_elephchtmlparser_m113())
        .method(decl_class_dom_elephchtmlparser_m114())
        .method(decl_class_dom_elephchtmlparser_m115())
        .method(decl_class_dom_elephchtmlparser_m116())
        .method(decl_class_dom_elephchtmlparser_m117())
        .method(decl_class_dom_elephchtmlparser_m118())
        .method(decl_class_dom_elephchtmlparser_m119())
        .method(decl_class_dom_elephchtmlparser_m120())
        .method(decl_class_dom_elephchtmlparser_m121())
        .method(decl_class_dom_elephchtmlparser_m122())
        .method(decl_class_dom_elephchtmlparser_m123())
        .method(decl_class_dom_elephchtmlparser_m124())
        .method(decl_class_dom_elephchtmlparser_m125())
        .method(decl_class_dom_elephchtmlparser_m126())
        .method(decl_class_dom_elephchtmlparser_m127())
        .method(decl_class_dom_elephchtmlparser_m128())
        .method(decl_class_dom_elephchtmlparser_m129())
        .method(decl_class_dom_elephchtmlparser_m130())
        .method(decl_class_dom_elephchtmlparser_m131())
        .method(decl_class_dom_elephchtmlparser_m132())
        .method(decl_class_dom_elephchtmlparser_m133())
        .method(decl_class_dom_elephchtmlparser_m134())
        .method(decl_class_dom_elephchtmlparser_m135())
        .method(decl_class_dom_elephchtmlparser_m136())
        .method(decl_class_dom_elephchtmlparser_m137())
        .method(decl_class_dom_elephchtmlparser_m138())
        .method(decl_class_dom_elephchtmlparser_m139())
        .method(decl_class_dom_elephchtmlparser_m140())
        .method(decl_class_dom_elephchtmlparser_m141())
        .method(decl_class_dom_elephchtmlparser_m142())
        .method(decl_class_dom_elephchtmlparser_m143())
        .method(decl_class_dom_elephchtmlparser_m144())
        .method(decl_class_dom_elephchtmlparser_m145())
        .method(decl_class_dom_elephchtmlparser_m146())
        .method(decl_class_dom_elephchtmlparser_m147())
        .method(decl_class_dom_elephchtmlparser_m148())
        .method(decl_class_dom_elephchtmlparser_m149())
        .method(decl_class_dom_elephchtmlparser_m150())
        .build()
}

const ENTITY_TABLE: &[(&str, &str)] = &[
    ("AElig;", "Æ"),
    ("AMP;", "&"),
    ("Aacute;", "Á"),
    ("Abreve;", "Ă"),
    ("Acirc;", "Â"),
    ("Acy;", "А"),
    ("Afr;", "𝔄"),
    ("Agrave;", "À"),
    ("Alpha;", "Α"),
    ("Amacr;", "Ā"),
    ("And;", "⩓"),
    ("Aogon;", "Ą"),
    ("Aopf;", "𝔸"),
    ("ApplyFunction;", "\u{2061}"),
    ("Aring;", "Å"),
    ("Ascr;", "𝒜"),
    ("Assign;", "≔"),
    ("Atilde;", "Ã"),
    ("Auml;", "Ä"),
    ("Backslash;", "∖"),
    ("Barv;", "⫧"),
    ("Barwed;", "⌆"),
    ("Bcy;", "Б"),
    ("Because;", "∵"),
    ("Bernoullis;", "ℬ"),
    ("Beta;", "Β"),
    ("Bfr;", "𝔅"),
    ("Bopf;", "𝔹"),
    ("Breve;", "˘"),
    ("Bscr;", "ℬ"),
    ("Bumpeq;", "≎"),
    ("CHcy;", "Ч"),
    ("COPY;", "©"),
    ("Cacute;", "Ć"),
    ("Cap;", "⋒"),
    ("CapitalDifferentialD;", "ⅅ"),
    ("Cayleys;", "ℭ"),
    ("Ccaron;", "Č"),
    ("Ccedil;", "Ç"),
    ("Ccirc;", "Ĉ"),
    ("Cconint;", "∰"),
    ("Cdot;", "Ċ"),
    ("Cedilla;", "¸"),
    ("CenterDot;", "·"),
    ("Cfr;", "ℭ"),
    ("Chi;", "Χ"),
    ("CircleDot;", "⊙"),
    ("CircleMinus;", "⊖"),
    ("CirclePlus;", "⊕"),
    ("CircleTimes;", "⊗"),
    ("ClockwiseContourIntegral;", "∲"),
    ("CloseCurlyDoubleQuote;", "”"),
    ("CloseCurlyQuote;", "’"),
    ("Colon;", "∷"),
    ("Colone;", "⩴"),
    ("Congruent;", "≡"),
    ("Conint;", "∯"),
    ("ContourIntegral;", "∮"),
    ("Copf;", "ℂ"),
    ("Coproduct;", "∐"),
    ("CounterClockwiseContourIntegral;", "∳"),
    ("Cross;", "⨯"),
    ("Cscr;", "𝒞"),
    ("Cup;", "⋓"),
    ("CupCap;", "≍"),
    ("DD;", "ⅅ"),
    ("DDotrahd;", "⤑"),
    ("DJcy;", "Ђ"),
    ("DScy;", "Ѕ"),
    ("DZcy;", "Џ"),
    ("Dagger;", "‡"),
    ("Darr;", "↡"),
    ("Dashv;", "⫤"),
    ("Dcaron;", "Ď"),
    ("Dcy;", "Д"),
    ("Del;", "∇"),
    ("Delta;", "Δ"),
    ("Dfr;", "𝔇"),
    ("DiacriticalAcute;", "´"),
    ("DiacriticalDot;", "˙"),
    ("DiacriticalDoubleAcute;", "˝"),
    ("DiacriticalGrave;", "`"),
    ("DiacriticalTilde;", "˜"),
    ("Diamond;", "⋄"),
    ("DifferentialD;", "ⅆ"),
    ("Dopf;", "𝔻"),
    ("Dot;", "¨"),
    ("DotDot;", "\u{20dc}"),
    ("DotEqual;", "≐"),
    ("DoubleContourIntegral;", "∯"),
    ("DoubleDot;", "¨"),
    ("DoubleDownArrow;", "⇓"),
    ("DoubleLeftArrow;", "⇐"),
    ("DoubleLeftRightArrow;", "⇔"),
    ("DoubleLeftTee;", "⫤"),
    ("DoubleLongLeftArrow;", "⟸"),
    ("DoubleLongLeftRightArrow;", "⟺"),
    ("DoubleLongRightArrow;", "⟹"),
    ("DoubleRightArrow;", "⇒"),
    ("DoubleRightTee;", "⊨"),
    ("DoubleUpArrow;", "⇑"),
    ("DoubleUpDownArrow;", "⇕"),
    ("DoubleVerticalBar;", "∥"),
    ("DownArrow;", "↓"),
    ("DownArrowBar;", "⤓"),
    ("DownArrowUpArrow;", "⇵"),
    ("DownBreve;", "\u{311}"),
    ("DownLeftRightVector;", "⥐"),
    ("DownLeftTeeVector;", "⥞"),
    ("DownLeftVector;", "↽"),
    ("DownLeftVectorBar;", "⥖"),
    ("DownRightTeeVector;", "⥟"),
    ("DownRightVector;", "⇁"),
    ("DownRightVectorBar;", "⥗"),
    ("DownTee;", "⊤"),
    ("DownTeeArrow;", "↧"),
    ("Downarrow;", "⇓"),
    ("Dscr;", "𝒟"),
    ("Dstrok;", "Đ"),
    ("ENG;", "Ŋ"),
    ("ETH;", "Ð"),
    ("Eacute;", "É"),
    ("Ecaron;", "Ě"),
    ("Ecirc;", "Ê"),
    ("Ecy;", "Э"),
    ("Edot;", "Ė"),
    ("Efr;", "𝔈"),
    ("Egrave;", "È"),
    ("Element;", "∈"),
    ("Emacr;", "Ē"),
    ("EmptySmallSquare;", "◻"),
    ("EmptyVerySmallSquare;", "▫"),
    ("Eogon;", "Ę"),
    ("Eopf;", "𝔼"),
    ("Epsilon;", "Ε"),
    ("Equal;", "⩵"),
    ("EqualTilde;", "≂"),
    ("Equilibrium;", "⇌"),
    ("Escr;", "ℰ"),
    ("Esim;", "⩳"),
    ("Eta;", "Η"),
    ("Euml;", "Ë"),
    ("Exists;", "∃"),
    ("ExponentialE;", "ⅇ"),
    ("Fcy;", "Ф"),
    ("Ffr;", "𝔉"),
    ("FilledSmallSquare;", "◼"),
    ("FilledVerySmallSquare;", "▪"),
    ("Fopf;", "𝔽"),
    ("ForAll;", "∀"),
    ("Fouriertrf;", "ℱ"),
    ("Fscr;", "ℱ"),
    ("GJcy;", "Ѓ"),
    ("GT;", ">"),
    ("Gamma;", "Γ"),
    ("Gammad;", "Ϝ"),
    ("Gbreve;", "Ğ"),
    ("Gcedil;", "Ģ"),
    ("Gcirc;", "Ĝ"),
    ("Gcy;", "Г"),
    ("Gdot;", "Ġ"),
    ("Gfr;", "𝔊"),
    ("Gg;", "⋙"),
    ("Gopf;", "𝔾"),
    ("GreaterEqual;", "≥"),
    ("GreaterEqualLess;", "⋛"),
    ("GreaterFullEqual;", "≧"),
    ("GreaterGreater;", "⪢"),
    ("GreaterLess;", "≷"),
    ("GreaterSlantEqual;", "⩾"),
    ("GreaterTilde;", "≳"),
    ("Gscr;", "𝒢"),
    ("Gt;", "≫"),
    ("HARDcy;", "Ъ"),
    ("Hacek;", "ˇ"),
    ("Hat;", "^"),
    ("Hcirc;", "Ĥ"),
    ("Hfr;", "ℌ"),
    ("HilbertSpace;", "ℋ"),
    ("Hopf;", "ℍ"),
    ("HorizontalLine;", "─"),
    ("Hscr;", "ℋ"),
    ("Hstrok;", "Ħ"),
    ("HumpDownHump;", "≎"),
    ("HumpEqual;", "≏"),
    ("IEcy;", "Е"),
    ("IJlig;", "Ĳ"),
    ("IOcy;", "Ё"),
    ("Iacute;", "Í"),
    ("Icirc;", "Î"),
    ("Icy;", "И"),
    ("Idot;", "İ"),
    ("Ifr;", "ℑ"),
    ("Igrave;", "Ì"),
    ("Im;", "ℑ"),
    ("Imacr;", "Ī"),
    ("ImaginaryI;", "ⅈ"),
    ("Implies;", "⇒"),
    ("Int;", "∬"),
    ("Integral;", "∫"),
    ("Intersection;", "⋂"),
    ("InvisibleComma;", "\u{2063}"),
    ("InvisibleTimes;", "\u{2062}"),
    ("Iogon;", "Į"),
    ("Iopf;", "𝕀"),
    ("Iota;", "Ι"),
    ("Iscr;", "ℐ"),
    ("Itilde;", "Ĩ"),
    ("Iukcy;", "І"),
    ("Iuml;", "Ï"),
    ("Jcirc;", "Ĵ"),
    ("Jcy;", "Й"),
    ("Jfr;", "𝔍"),
    ("Jopf;", "𝕁"),
    ("Jscr;", "𝒥"),
    ("Jsercy;", "Ј"),
    ("Jukcy;", "Є"),
    ("KHcy;", "Х"),
    ("KJcy;", "Ќ"),
    ("Kappa;", "Κ"),
    ("Kcedil;", "Ķ"),
    ("Kcy;", "К"),
    ("Kfr;", "𝔎"),
    ("Kopf;", "𝕂"),
    ("Kscr;", "𝒦"),
    ("LJcy;", "Љ"),
    ("LT;", "<"),
    ("Lacute;", "Ĺ"),
    ("Lambda;", "Λ"),
    ("Lang;", "⟪"),
    ("Laplacetrf;", "ℒ"),
    ("Larr;", "↞"),
    ("Lcaron;", "Ľ"),
    ("Lcedil;", "Ļ"),
    ("Lcy;", "Л"),
    ("LeftAngleBracket;", "⟨"),
    ("LeftArrow;", "←"),
    ("LeftArrowBar;", "⇤"),
    ("LeftArrowRightArrow;", "⇆"),
    ("LeftCeiling;", "⌈"),
    ("LeftDoubleBracket;", "⟦"),
    ("LeftDownTeeVector;", "⥡"),
    ("LeftDownVector;", "⇃"),
    ("LeftDownVectorBar;", "⥙"),
    ("LeftFloor;", "⌊"),
    ("LeftRightArrow;", "↔"),
    ("LeftRightVector;", "⥎"),
    ("LeftTee;", "⊣"),
    ("LeftTeeArrow;", "↤"),
    ("LeftTeeVector;", "⥚"),
    ("LeftTriangle;", "⊲"),
    ("LeftTriangleBar;", "⧏"),
    ("LeftTriangleEqual;", "⊴"),
    ("LeftUpDownVector;", "⥑"),
    ("LeftUpTeeVector;", "⥠"),
    ("LeftUpVector;", "↿"),
    ("LeftUpVectorBar;", "⥘"),
    ("LeftVector;", "↼"),
    ("LeftVectorBar;", "⥒"),
    ("Leftarrow;", "⇐"),
    ("Leftrightarrow;", "⇔"),
    ("LessEqualGreater;", "⋚"),
    ("LessFullEqual;", "≦"),
    ("LessGreater;", "≶"),
    ("LessLess;", "⪡"),
    ("LessSlantEqual;", "⩽"),
    ("LessTilde;", "≲"),
    ("Lfr;", "𝔏"),
    ("Ll;", "⋘"),
    ("Lleftarrow;", "⇚"),
    ("Lmidot;", "Ŀ"),
    ("LongLeftArrow;", "⟵"),
    ("LongLeftRightArrow;", "⟷"),
    ("LongRightArrow;", "⟶"),
    ("Longleftarrow;", "⟸"),
    ("Longleftrightarrow;", "⟺"),
    ("Longrightarrow;", "⟹"),
    ("Lopf;", "𝕃"),
    ("LowerLeftArrow;", "↙"),
    ("LowerRightArrow;", "↘"),
    ("Lscr;", "ℒ"),
    ("Lsh;", "↰"),
    ("Lstrok;", "Ł"),
    ("Lt;", "≪"),
    ("Map;", "⤅"),
    ("Mcy;", "М"),
    ("MediumSpace;", "\u{205f}"),
    ("Mellintrf;", "ℳ"),
    ("Mfr;", "𝔐"),
    ("MinusPlus;", "∓"),
    ("Mopf;", "𝕄"),
    ("Mscr;", "ℳ"),
    ("Mu;", "Μ"),
    ("NJcy;", "Њ"),
    ("Nacute;", "Ń"),
    ("Ncaron;", "Ň"),
    ("Ncedil;", "Ņ"),
    ("Ncy;", "Н"),
    ("NegativeMediumSpace;", "\u{200b}"),
    ("NegativeThickSpace;", "\u{200b}"),
    ("NegativeThinSpace;", "\u{200b}"),
    ("NegativeVeryThinSpace;", "\u{200b}"),
    ("NestedGreaterGreater;", "≫"),
    ("NestedLessLess;", "≪"),
    ("NewLine;", "\n"),
    ("Nfr;", "𝔑"),
    ("NoBreak;", "\u{2060}"),
    ("NonBreakingSpace;", "\u{a0}"),
    ("Nopf;", "ℕ"),
    ("Not;", "⫬"),
    ("NotCongruent;", "≢"),
    ("NotCupCap;", "≭"),
    ("NotDoubleVerticalBar;", "∦"),
    ("NotElement;", "∉"),
    ("NotEqual;", "≠"),
    ("NotEqualTilde;", "≂\u{338}"),
    ("NotExists;", "∄"),
    ("NotGreater;", "≯"),
    ("NotGreaterEqual;", "≱"),
    ("NotGreaterFullEqual;", "≧\u{338}"),
    ("NotGreaterGreater;", "≫\u{338}"),
    ("NotGreaterLess;", "≹"),
    ("NotGreaterSlantEqual;", "⩾\u{338}"),
    ("NotGreaterTilde;", "≵"),
    ("NotHumpDownHump;", "≎\u{338}"),
    ("NotHumpEqual;", "≏\u{338}"),
    ("NotLeftTriangle;", "⋪"),
    ("NotLeftTriangleBar;", "⧏\u{338}"),
    ("NotLeftTriangleEqual;", "⋬"),
    ("NotLess;", "≮"),
    ("NotLessEqual;", "≰"),
    ("NotLessGreater;", "≸"),
    ("NotLessLess;", "≪\u{338}"),
    ("NotLessSlantEqual;", "⩽\u{338}"),
    ("NotLessTilde;", "≴"),
    ("NotNestedGreaterGreater;", "⪢\u{338}"),
    ("NotNestedLessLess;", "⪡\u{338}"),
    ("NotPrecedes;", "⊀"),
    ("NotPrecedesEqual;", "⪯\u{338}"),
    ("NotPrecedesSlantEqual;", "⋠"),
    ("NotReverseElement;", "∌"),
    ("NotRightTriangle;", "⋫"),
    ("NotRightTriangleBar;", "⧐\u{338}"),
    ("NotRightTriangleEqual;", "⋭"),
    ("NotSquareSubset;", "⊏\u{338}"),
    ("NotSquareSubsetEqual;", "⋢"),
    ("NotSquareSuperset;", "⊐\u{338}"),
    ("NotSquareSupersetEqual;", "⋣"),
    ("NotSubset;", "⊂\u{20d2}"),
    ("NotSubsetEqual;", "⊈"),
    ("NotSucceeds;", "⊁"),
    ("NotSucceedsEqual;", "⪰\u{338}"),
    ("NotSucceedsSlantEqual;", "⋡"),
    ("NotSucceedsTilde;", "≿\u{338}"),
    ("NotSuperset;", "⊃\u{20d2}"),
    ("NotSupersetEqual;", "⊉"),
    ("NotTilde;", "≁"),
    ("NotTildeEqual;", "≄"),
    ("NotTildeFullEqual;", "≇"),
    ("NotTildeTilde;", "≉"),
    ("NotVerticalBar;", "∤"),
    ("Nscr;", "𝒩"),
    ("Ntilde;", "Ñ"),
    ("Nu;", "Ν"),
    ("OElig;", "Œ"),
    ("Oacute;", "Ó"),
    ("Ocirc;", "Ô"),
    ("Ocy;", "О"),
    ("Odblac;", "Ő"),
    ("Ofr;", "𝔒"),
    ("Ograve;", "Ò"),
    ("Omacr;", "Ō"),
    ("Omega;", "Ω"),
    ("Omicron;", "Ο"),
    ("Oopf;", "𝕆"),
    ("OpenCurlyDoubleQuote;", "“"),
    ("OpenCurlyQuote;", "‘"),
    ("Or;", "⩔"),
    ("Oscr;", "𝒪"),
    ("Oslash;", "Ø"),
    ("Otilde;", "Õ"),
    ("Otimes;", "⨷"),
    ("Ouml;", "Ö"),
    ("OverBar;", "‾"),
    ("OverBrace;", "⏞"),
    ("OverBracket;", "⎴"),
    ("OverParenthesis;", "⏜"),
    ("PartialD;", "∂"),
    ("Pcy;", "П"),
    ("Pfr;", "𝔓"),
    ("Phi;", "Φ"),
    ("Pi;", "Π"),
    ("PlusMinus;", "±"),
    ("Poincareplane;", "ℌ"),
    ("Popf;", "ℙ"),
    ("Pr;", "⪻"),
    ("Precedes;", "≺"),
    ("PrecedesEqual;", "⪯"),
    ("PrecedesSlantEqual;", "≼"),
    ("PrecedesTilde;", "≾"),
    ("Prime;", "″"),
    ("Product;", "∏"),
    ("Proportion;", "∷"),
    ("Proportional;", "∝"),
    ("Pscr;", "𝒫"),
    ("Psi;", "Ψ"),
    ("QUOT;", "\""),
    ("Qfr;", "𝔔"),
    ("Qopf;", "ℚ"),
    ("Qscr;", "𝒬"),
    ("RBarr;", "⤐"),
    ("REG;", "®"),
    ("Racute;", "Ŕ"),
    ("Rang;", "⟫"),
    ("Rarr;", "↠"),
    ("Rarrtl;", "⤖"),
    ("Rcaron;", "Ř"),
    ("Rcedil;", "Ŗ"),
    ("Rcy;", "Р"),
    ("Re;", "ℜ"),
    ("ReverseElement;", "∋"),
    ("ReverseEquilibrium;", "⇋"),
    ("ReverseUpEquilibrium;", "⥯"),
    ("Rfr;", "ℜ"),
    ("Rho;", "Ρ"),
    ("RightAngleBracket;", "⟩"),
    ("RightArrow;", "→"),
    ("RightArrowBar;", "⇥"),
    ("RightArrowLeftArrow;", "⇄"),
    ("RightCeiling;", "⌉"),
    ("RightDoubleBracket;", "⟧"),
    ("RightDownTeeVector;", "⥝"),
    ("RightDownVector;", "⇂"),
    ("RightDownVectorBar;", "⥕"),
    ("RightFloor;", "⌋"),
    ("RightTee;", "⊢"),
    ("RightTeeArrow;", "↦"),
    ("RightTeeVector;", "⥛"),
    ("RightTriangle;", "⊳"),
    ("RightTriangleBar;", "⧐"),
    ("RightTriangleEqual;", "⊵"),
    ("RightUpDownVector;", "⥏"),
    ("RightUpTeeVector;", "⥜"),
    ("RightUpVector;", "↾"),
    ("RightUpVectorBar;", "⥔"),
    ("RightVector;", "⇀"),
    ("RightVectorBar;", "⥓"),
    ("Rightarrow;", "⇒"),
    ("Ropf;", "ℝ"),
    ("RoundImplies;", "⥰"),
    ("Rrightarrow;", "⇛"),
    ("Rscr;", "ℛ"),
    ("Rsh;", "↱"),
    ("RuleDelayed;", "⧴"),
    ("SHCHcy;", "Щ"),
    ("SHcy;", "Ш"),
    ("SOFTcy;", "Ь"),
    ("Sacute;", "Ś"),
    ("Sc;", "⪼"),
    ("Scaron;", "Š"),
    ("Scedil;", "Ş"),
    ("Scirc;", "Ŝ"),
    ("Scy;", "С"),
    ("Sfr;", "𝔖"),
    ("ShortDownArrow;", "↓"),
    ("ShortLeftArrow;", "←"),
    ("ShortRightArrow;", "→"),
    ("ShortUpArrow;", "↑"),
    ("Sigma;", "Σ"),
    ("SmallCircle;", "∘"),
    ("Sopf;", "𝕊"),
    ("Sqrt;", "√"),
    ("Square;", "□"),
    ("SquareIntersection;", "⊓"),
    ("SquareSubset;", "⊏"),
    ("SquareSubsetEqual;", "⊑"),
    ("SquareSuperset;", "⊐"),
    ("SquareSupersetEqual;", "⊒"),
    ("SquareUnion;", "⊔"),
    ("Sscr;", "𝒮"),
    ("Star;", "⋆"),
    ("Sub;", "⋐"),
    ("Subset;", "⋐"),
    ("SubsetEqual;", "⊆"),
    ("Succeeds;", "≻"),
    ("SucceedsEqual;", "⪰"),
    ("SucceedsSlantEqual;", "≽"),
    ("SucceedsTilde;", "≿"),
    ("SuchThat;", "∋"),
    ("Sum;", "∑"),
    ("Sup;", "⋑"),
    ("Superset;", "⊃"),
    ("SupersetEqual;", "⊇"),
    ("Supset;", "⋑"),
    ("THORN;", "Þ"),
    ("TRADE;", "™"),
    ("TSHcy;", "Ћ"),
    ("TScy;", "Ц"),
    ("Tab;", "\t"),
    ("Tau;", "Τ"),
    ("Tcaron;", "Ť"),
    ("Tcedil;", "Ţ"),
    ("Tcy;", "Т"),
    ("Tfr;", "𝔗"),
    ("Therefore;", "∴"),
    ("Theta;", "Θ"),
    ("ThickSpace;", "\u{205f}\u{200a}"),
    ("ThinSpace;", "\u{2009}"),
    ("Tilde;", "∼"),
    ("TildeEqual;", "≃"),
    ("TildeFullEqual;", "≅"),
    ("TildeTilde;", "≈"),
    ("Topf;", "𝕋"),
    ("TripleDot;", "\u{20db}"),
    ("Tscr;", "𝒯"),
    ("Tstrok;", "Ŧ"),
    ("Uacute;", "Ú"),
    ("Uarr;", "↟"),
    ("Uarrocir;", "⥉"),
    ("Ubrcy;", "Ў"),
    ("Ubreve;", "Ŭ"),
    ("Ucirc;", "Û"),
    ("Ucy;", "У"),
    ("Udblac;", "Ű"),
    ("Ufr;", "𝔘"),
    ("Ugrave;", "Ù"),
    ("Umacr;", "Ū"),
    ("UnderBar;", "_"),
    ("UnderBrace;", "⏟"),
    ("UnderBracket;", "⎵"),
    ("UnderParenthesis;", "⏝"),
    ("Union;", "⋃"),
    ("UnionPlus;", "⊎"),
    ("Uogon;", "Ų"),
    ("Uopf;", "𝕌"),
    ("UpArrow;", "↑"),
    ("UpArrowBar;", "⤒"),
    ("UpArrowDownArrow;", "⇅"),
    ("UpDownArrow;", "↕"),
    ("UpEquilibrium;", "⥮"),
    ("UpTee;", "⊥"),
    ("UpTeeArrow;", "↥"),
    ("Uparrow;", "⇑"),
    ("Updownarrow;", "⇕"),
    ("UpperLeftArrow;", "↖"),
    ("UpperRightArrow;", "↗"),
    ("Upsi;", "ϒ"),
    ("Upsilon;", "Υ"),
    ("Uring;", "Ů"),
    ("Uscr;", "𝒰"),
    ("Utilde;", "Ũ"),
    ("Uuml;", "Ü"),
    ("VDash;", "⊫"),
    ("Vbar;", "⫫"),
    ("Vcy;", "В"),
    ("Vdash;", "⊩"),
    ("Vdashl;", "⫦"),
    ("Vee;", "⋁"),
    ("Verbar;", "‖"),
    ("Vert;", "‖"),
    ("VerticalBar;", "∣"),
    ("VerticalLine;", "|"),
    ("VerticalSeparator;", "❘"),
    ("VerticalTilde;", "≀"),
    ("VeryThinSpace;", "\u{200a}"),
    ("Vfr;", "𝔙"),
    ("Vopf;", "𝕍"),
    ("Vscr;", "𝒱"),
    ("Vvdash;", "⊪"),
    ("Wcirc;", "Ŵ"),
    ("Wedge;", "⋀"),
    ("Wfr;", "𝔚"),
    ("Wopf;", "𝕎"),
    ("Wscr;", "𝒲"),
    ("Xfr;", "𝔛"),
    ("Xi;", "Ξ"),
    ("Xopf;", "𝕏"),
    ("Xscr;", "𝒳"),
    ("YAcy;", "Я"),
    ("YIcy;", "Ї"),
    ("YUcy;", "Ю"),
    ("Yacute;", "Ý"),
    ("Ycirc;", "Ŷ"),
    ("Ycy;", "Ы"),
    ("Yfr;", "𝔜"),
    ("Yopf;", "𝕐"),
    ("Yscr;", "𝒴"),
    ("Yuml;", "Ÿ"),
    ("ZHcy;", "Ж"),
    ("Zacute;", "Ź"),
    ("Zcaron;", "Ž"),
    ("Zcy;", "З"),
    ("Zdot;", "Ż"),
    ("ZeroWidthSpace;", "\u{200b}"),
    ("Zeta;", "Ζ"),
    ("Zfr;", "ℨ"),
    ("Zopf;", "ℤ"),
    ("Zscr;", "𝒵"),
    ("aacute;", "á"),
    ("abreve;", "ă"),
    ("ac;", "∾"),
    ("acE;", "∾\u{333}"),
    ("acd;", "∿"),
    ("acirc;", "â"),
    ("acute;", "´"),
    ("acy;", "а"),
    ("aelig;", "æ"),
    ("af;", "\u{2061}"),
    ("afr;", "𝔞"),
    ("agrave;", "à"),
    ("alefsym;", "ℵ"),
    ("aleph;", "ℵ"),
    ("alpha;", "α"),
    ("amacr;", "ā"),
    ("amalg;", "⨿"),
    ("amp;", "&"),
    ("and;", "∧"),
    ("andand;", "⩕"),
    ("andd;", "⩜"),
    ("andslope;", "⩘"),
    ("andv;", "⩚"),
    ("ang;", "∠"),
    ("ange;", "⦤"),
    ("angle;", "∠"),
    ("angmsd;", "∡"),
    ("angmsdaa;", "⦨"),
    ("angmsdab;", "⦩"),
    ("angmsdac;", "⦪"),
    ("angmsdad;", "⦫"),
    ("angmsdae;", "⦬"),
    ("angmsdaf;", "⦭"),
    ("angmsdag;", "⦮"),
    ("angmsdah;", "⦯"),
    ("angrt;", "∟"),
    ("angrtvb;", "⊾"),
    ("angrtvbd;", "⦝"),
    ("angsph;", "∢"),
    ("angst;", "Å"),
    ("angzarr;", "⍼"),
    ("aogon;", "ą"),
    ("aopf;", "𝕒"),
    ("ap;", "≈"),
    ("apE;", "⩰"),
    ("apacir;", "⩯"),
    ("ape;", "≊"),
    ("apid;", "≋"),
    ("apos;", "'"),
    ("approx;", "≈"),
    ("approxeq;", "≊"),
    ("aring;", "å"),
    ("ascr;", "𝒶"),
    ("ast;", "*"),
    ("asymp;", "≈"),
    ("asympeq;", "≍"),
    ("atilde;", "ã"),
    ("auml;", "ä"),
    ("awconint;", "∳"),
    ("awint;", "⨑"),
    ("bNot;", "⫭"),
    ("backcong;", "≌"),
    ("backepsilon;", "϶"),
    ("backprime;", "‵"),
    ("backsim;", "∽"),
    ("backsimeq;", "⋍"),
    ("barvee;", "⊽"),
    ("barwed;", "⌅"),
    ("barwedge;", "⌅"),
    ("bbrk;", "⎵"),
    ("bbrktbrk;", "⎶"),
    ("bcong;", "≌"),
    ("bcy;", "б"),
    ("bdquo;", "„"),
    ("becaus;", "∵"),
    ("because;", "∵"),
    ("bemptyv;", "⦰"),
    ("bepsi;", "϶"),
    ("bernou;", "ℬ"),
    ("beta;", "β"),
    ("beth;", "ℶ"),
    ("between;", "≬"),
    ("bfr;", "𝔟"),
    ("bigcap;", "⋂"),
    ("bigcirc;", "◯"),
    ("bigcup;", "⋃"),
    ("bigodot;", "⨀"),
    ("bigoplus;", "⨁"),
    ("bigotimes;", "⨂"),
    ("bigsqcup;", "⨆"),
    ("bigstar;", "★"),
    ("bigtriangledown;", "▽"),
    ("bigtriangleup;", "△"),
    ("biguplus;", "⨄"),
    ("bigvee;", "⋁"),
    ("bigwedge;", "⋀"),
    ("bkarow;", "⤍"),
    ("blacklozenge;", "⧫"),
    ("blacksquare;", "▪"),
    ("blacktriangle;", "▴"),
    ("blacktriangledown;", "▾"),
    ("blacktriangleleft;", "◂"),
    ("blacktriangleright;", "▸"),
    ("blank;", "␣"),
    ("blk12;", "▒"),
    ("blk14;", "░"),
    ("blk34;", "▓"),
    ("block;", "█"),
    ("bne;", "=\u{20e5}"),
    ("bnequiv;", "≡\u{20e5}"),
    ("bnot;", "⌐"),
    ("bopf;", "𝕓"),
    ("bot;", "⊥"),
    ("bottom;", "⊥"),
    ("bowtie;", "⋈"),
    ("boxDL;", "╗"),
    ("boxDR;", "╔"),
    ("boxDl;", "╖"),
    ("boxDr;", "╓"),
    ("boxH;", "═"),
    ("boxHD;", "╦"),
    ("boxHU;", "╩"),
    ("boxHd;", "╤"),
    ("boxHu;", "╧"),
    ("boxUL;", "╝"),
    ("boxUR;", "╚"),
    ("boxUl;", "╜"),
    ("boxUr;", "╙"),
    ("boxV;", "║"),
    ("boxVH;", "╬"),
    ("boxVL;", "╣"),
    ("boxVR;", "╠"),
    ("boxVh;", "╫"),
    ("boxVl;", "╢"),
    ("boxVr;", "╟"),
    ("boxbox;", "⧉"),
    ("boxdL;", "╕"),
    ("boxdR;", "╒"),
    ("boxdl;", "┐"),
    ("boxdr;", "┌"),
    ("boxh;", "─"),
    ("boxhD;", "╥"),
    ("boxhU;", "╨"),
    ("boxhd;", "┬"),
    ("boxhu;", "┴"),
    ("boxminus;", "⊟"),
    ("boxplus;", "⊞"),
    ("boxtimes;", "⊠"),
    ("boxuL;", "╛"),
    ("boxuR;", "╘"),
    ("boxul;", "┘"),
    ("boxur;", "└"),
    ("boxv;", "│"),
    ("boxvH;", "╪"),
    ("boxvL;", "╡"),
    ("boxvR;", "╞"),
    ("boxvh;", "┼"),
    ("boxvl;", "┤"),
    ("boxvr;", "├"),
    ("bprime;", "‵"),
    ("breve;", "˘"),
    ("brvbar;", "¦"),
    ("bscr;", "𝒷"),
    ("bsemi;", "⁏"),
    ("bsim;", "∽"),
    ("bsime;", "⋍"),
    ("bsol;", "\\"),
    ("bsolb;", "⧅"),
    ("bsolhsub;", "⟈"),
    ("bull;", "•"),
    ("bullet;", "•"),
    ("bump;", "≎"),
    ("bumpE;", "⪮"),
    ("bumpe;", "≏"),
    ("bumpeq;", "≏"),
    ("cacute;", "ć"),
    ("cap;", "∩"),
    ("capand;", "⩄"),
    ("capbrcup;", "⩉"),
    ("capcap;", "⩋"),
    ("capcup;", "⩇"),
    ("capdot;", "⩀"),
    ("caps;", "∩\u{fe00}"),
    ("caret;", "⁁"),
    ("caron;", "ˇ"),
    ("ccaps;", "⩍"),
    ("ccaron;", "č"),
    ("ccedil;", "ç"),
    ("ccirc;", "ĉ"),
    ("ccups;", "⩌"),
    ("ccupssm;", "⩐"),
    ("cdot;", "ċ"),
    ("cedil;", "¸"),
    ("cemptyv;", "⦲"),
    ("cent;", "¢"),
    ("centerdot;", "·"),
    ("cfr;", "𝔠"),
    ("chcy;", "ч"),
    ("check;", "✓"),
    ("checkmark;", "✓"),
    ("chi;", "χ"),
    ("cir;", "○"),
    ("cirE;", "⧃"),
    ("circ;", "ˆ"),
    ("circeq;", "≗"),
    ("circlearrowleft;", "↺"),
    ("circlearrowright;", "↻"),
    ("circledR;", "®"),
    ("circledS;", "Ⓢ"),
    ("circledast;", "⊛"),
    ("circledcirc;", "⊚"),
    ("circleddash;", "⊝"),
    ("cire;", "≗"),
    ("cirfnint;", "⨐"),
    ("cirmid;", "⫯"),
    ("cirscir;", "⧂"),
    ("clubs;", "♣"),
    ("clubsuit;", "♣"),
    ("colon;", ":"),
    ("colone;", "≔"),
    ("coloneq;", "≔"),
    ("comma;", ","),
    ("commat;", "@"),
    ("comp;", "∁"),
    ("compfn;", "∘"),
    ("complement;", "∁"),
    ("complexes;", "ℂ"),
    ("cong;", "≅"),
    ("congdot;", "⩭"),
    ("conint;", "∮"),
    ("copf;", "𝕔"),
    ("coprod;", "∐"),
    ("copy;", "©"),
    ("copysr;", "℗"),
    ("crarr;", "↵"),
    ("cross;", "✗"),
    ("cscr;", "𝒸"),
    ("csub;", "⫏"),
    ("csube;", "⫑"),
    ("csup;", "⫐"),
    ("csupe;", "⫒"),
    ("ctdot;", "⋯"),
    ("cudarrl;", "⤸"),
    ("cudarrr;", "⤵"),
    ("cuepr;", "⋞"),
    ("cuesc;", "⋟"),
    ("cularr;", "↶"),
    ("cularrp;", "⤽"),
    ("cup;", "∪"),
    ("cupbrcap;", "⩈"),
    ("cupcap;", "⩆"),
    ("cupcup;", "⩊"),
    ("cupdot;", "⊍"),
    ("cupor;", "⩅"),
    ("cups;", "∪\u{fe00}"),
    ("curarr;", "↷"),
    ("curarrm;", "⤼"),
    ("curlyeqprec;", "⋞"),
    ("curlyeqsucc;", "⋟"),
    ("curlyvee;", "⋎"),
    ("curlywedge;", "⋏"),
    ("curren;", "¤"),
    ("curvearrowleft;", "↶"),
    ("curvearrowright;", "↷"),
    ("cuvee;", "⋎"),
    ("cuwed;", "⋏"),
    ("cwconint;", "∲"),
    ("cwint;", "∱"),
    ("cylcty;", "⌭"),
    ("dArr;", "⇓"),
    ("dHar;", "⥥"),
    ("dagger;", "†"),
    ("daleth;", "ℸ"),
    ("darr;", "↓"),
    ("dash;", "‐"),
    ("dashv;", "⊣"),
    ("dbkarow;", "⤏"),
    ("dblac;", "˝"),
    ("dcaron;", "ď"),
    ("dcy;", "д"),
    ("dd;", "ⅆ"),
    ("ddagger;", "‡"),
    ("ddarr;", "⇊"),
    ("ddotseq;", "⩷"),
    ("deg;", "°"),
    ("delta;", "δ"),
    ("demptyv;", "⦱"),
    ("dfisht;", "⥿"),
    ("dfr;", "𝔡"),
    ("dharl;", "⇃"),
    ("dharr;", "⇂"),
    ("diam;", "⋄"),
    ("diamond;", "⋄"),
    ("diamondsuit;", "♦"),
    ("diams;", "♦"),
    ("die;", "¨"),
    ("digamma;", "ϝ"),
    ("disin;", "⋲"),
    ("div;", "÷"),
    ("divide;", "÷"),
    ("divideontimes;", "⋇"),
    ("divonx;", "⋇"),
    ("djcy;", "ђ"),
    ("dlcorn;", "⌞"),
    ("dlcrop;", "⌍"),
    ("dollar;", "$"),
    ("dopf;", "𝕕"),
    ("dot;", "˙"),
    ("doteq;", "≐"),
    ("doteqdot;", "≑"),
    ("dotminus;", "∸"),
    ("dotplus;", "∔"),
    ("dotsquare;", "⊡"),
    ("doublebarwedge;", "⌆"),
    ("downarrow;", "↓"),
    ("downdownarrows;", "⇊"),
    ("downharpoonleft;", "⇃"),
    ("downharpoonright;", "⇂"),
    ("drbkarow;", "⤐"),
    ("drcorn;", "⌟"),
    ("drcrop;", "⌌"),
    ("dscr;", "𝒹"),
    ("dscy;", "ѕ"),
    ("dsol;", "⧶"),
    ("dstrok;", "đ"),
    ("dtdot;", "⋱"),
    ("dtri;", "▿"),
    ("dtrif;", "▾"),
    ("duarr;", "⇵"),
    ("duhar;", "⥯"),
    ("dwangle;", "⦦"),
    ("dzcy;", "џ"),
    ("dzigrarr;", "⟿"),
    ("eDDot;", "⩷"),
    ("eDot;", "≑"),
    ("eacute;", "é"),
    ("easter;", "⩮"),
    ("ecaron;", "ě"),
    ("ecir;", "≖"),
    ("ecirc;", "ê"),
    ("ecolon;", "≕"),
    ("ecy;", "э"),
    ("edot;", "ė"),
    ("ee;", "ⅇ"),
    ("efDot;", "≒"),
    ("efr;", "𝔢"),
    ("eg;", "⪚"),
    ("egrave;", "è"),
    ("egs;", "⪖"),
    ("egsdot;", "⪘"),
    ("el;", "⪙"),
    ("elinters;", "⏧"),
    ("ell;", "ℓ"),
    ("els;", "⪕"),
    ("elsdot;", "⪗"),
    ("emacr;", "ē"),
    ("empty;", "∅"),
    ("emptyset;", "∅"),
    ("emptyv;", "∅"),
    ("emsp13;", "\u{2004}"),
    ("emsp14;", "\u{2005}"),
    ("emsp;", "\u{2003}"),
    ("eng;", "ŋ"),
    ("ensp;", "\u{2002}"),
    ("eogon;", "ę"),
    ("eopf;", "𝕖"),
    ("epar;", "⋕"),
    ("eparsl;", "⧣"),
    ("eplus;", "⩱"),
    ("epsi;", "ε"),
    ("epsilon;", "ε"),
    ("epsiv;", "ϵ"),
    ("eqcirc;", "≖"),
    ("eqcolon;", "≕"),
    ("eqsim;", "≂"),
    ("eqslantgtr;", "⪖"),
    ("eqslantless;", "⪕"),
    ("equals;", "="),
    ("equest;", "≟"),
    ("equiv;", "≡"),
    ("equivDD;", "⩸"),
    ("eqvparsl;", "⧥"),
    ("erDot;", "≓"),
    ("erarr;", "⥱"),
    ("escr;", "ℯ"),
    ("esdot;", "≐"),
    ("esim;", "≂"),
    ("eta;", "η"),
    ("eth;", "ð"),
    ("euml;", "ë"),
    ("euro;", "€"),
    ("excl;", "!"),
    ("exist;", "∃"),
    ("expectation;", "ℰ"),
    ("exponentiale;", "ⅇ"),
    ("fallingdotseq;", "≒"),
    ("fcy;", "ф"),
    ("female;", "♀"),
    ("ffilig;", "ﬃ"),
    ("fflig;", "ﬀ"),
    ("ffllig;", "ﬄ"),
    ("ffr;", "𝔣"),
    ("filig;", "ﬁ"),
    ("fjlig;", "fj"),
    ("flat;", "♭"),
    ("fllig;", "ﬂ"),
    ("fltns;", "▱"),
    ("fnof;", "ƒ"),
    ("fopf;", "𝕗"),
    ("forall;", "∀"),
    ("fork;", "⋔"),
    ("forkv;", "⫙"),
    ("fpartint;", "⨍"),
    ("frac12;", "½"),
    ("frac13;", "⅓"),
    ("frac14;", "¼"),
    ("frac15;", "⅕"),
    ("frac16;", "⅙"),
    ("frac18;", "⅛"),
    ("frac23;", "⅔"),
    ("frac25;", "⅖"),
    ("frac34;", "¾"),
    ("frac35;", "⅗"),
    ("frac38;", "⅜"),
    ("frac45;", "⅘"),
    ("frac56;", "⅚"),
    ("frac58;", "⅝"),
    ("frac78;", "⅞"),
    ("frasl;", "⁄"),
    ("frown;", "⌢"),
    ("fscr;", "𝒻"),
    ("gE;", "≧"),
    ("gEl;", "⪌"),
    ("gacute;", "ǵ"),
    ("gamma;", "γ"),
    ("gammad;", "ϝ"),
    ("gap;", "⪆"),
    ("gbreve;", "ğ"),
    ("gcirc;", "ĝ"),
    ("gcy;", "г"),
    ("gdot;", "ġ"),
    ("ge;", "≥"),
    ("gel;", "⋛"),
    ("geq;", "≥"),
    ("geqq;", "≧"),
    ("geqslant;", "⩾"),
    ("ges;", "⩾"),
    ("gescc;", "⪩"),
    ("gesdot;", "⪀"),
    ("gesdoto;", "⪂"),
    ("gesdotol;", "⪄"),
    ("gesl;", "⋛\u{fe00}"),
    ("gesles;", "⪔"),
    ("gfr;", "𝔤"),
    ("gg;", "≫"),
    ("ggg;", "⋙"),
    ("gimel;", "ℷ"),
    ("gjcy;", "ѓ"),
    ("gl;", "≷"),
    ("glE;", "⪒"),
    ("gla;", "⪥"),
    ("glj;", "⪤"),
    ("gnE;", "≩"),
    ("gnap;", "⪊"),
    ("gnapprox;", "⪊"),
    ("gne;", "⪈"),
    ("gneq;", "⪈"),
    ("gneqq;", "≩"),
    ("gnsim;", "⋧"),
    ("gopf;", "𝕘"),
    ("grave;", "`"),
    ("gscr;", "ℊ"),
    ("gsim;", "≳"),
    ("gsime;", "⪎"),
    ("gsiml;", "⪐"),
    ("gt;", ">"),
    ("gtcc;", "⪧"),
    ("gtcir;", "⩺"),
    ("gtdot;", "⋗"),
    ("gtlPar;", "⦕"),
    ("gtquest;", "⩼"),
    ("gtrapprox;", "⪆"),
    ("gtrarr;", "⥸"),
    ("gtrdot;", "⋗"),
    ("gtreqless;", "⋛"),
    ("gtreqqless;", "⪌"),
    ("gtrless;", "≷"),
    ("gtrsim;", "≳"),
    ("gvertneqq;", "≩\u{fe00}"),
    ("gvnE;", "≩\u{fe00}"),
    ("hArr;", "⇔"),
    ("hairsp;", "\u{200a}"),
    ("half;", "½"),
    ("hamilt;", "ℋ"),
    ("hardcy;", "ъ"),
    ("harr;", "↔"),
    ("harrcir;", "⥈"),
    ("harrw;", "↭"),
    ("hbar;", "ℏ"),
    ("hcirc;", "ĥ"),
    ("hearts;", "♥"),
    ("heartsuit;", "♥"),
    ("hellip;", "…"),
    ("hercon;", "⊹"),
    ("hfr;", "𝔥"),
    ("hksearow;", "⤥"),
    ("hkswarow;", "⤦"),
    ("hoarr;", "⇿"),
    ("homtht;", "∻"),
    ("hookleftarrow;", "↩"),
    ("hookrightarrow;", "↪"),
    ("hopf;", "𝕙"),
    ("horbar;", "―"),
    ("hscr;", "𝒽"),
    ("hslash;", "ℏ"),
    ("hstrok;", "ħ"),
    ("hybull;", "⁃"),
    ("hyphen;", "‐"),
    ("iacute;", "í"),
    ("ic;", "\u{2063}"),
    ("icirc;", "î"),
    ("icy;", "и"),
    ("iecy;", "е"),
    ("iexcl;", "¡"),
    ("iff;", "⇔"),
    ("ifr;", "𝔦"),
    ("igrave;", "ì"),
    ("ii;", "ⅈ"),
    ("iiiint;", "⨌"),
    ("iiint;", "∭"),
    ("iinfin;", "⧜"),
    ("iiota;", "℩"),
    ("ijlig;", "ĳ"),
    ("imacr;", "ī"),
    ("image;", "ℑ"),
    ("imagline;", "ℐ"),
    ("imagpart;", "ℑ"),
    ("imath;", "ı"),
    ("imof;", "⊷"),
    ("imped;", "Ƶ"),
    ("in;", "∈"),
    ("incare;", "℅"),
    ("infin;", "∞"),
    ("infintie;", "⧝"),
    ("inodot;", "ı"),
    ("int;", "∫"),
    ("intcal;", "⊺"),
    ("integers;", "ℤ"),
    ("intercal;", "⊺"),
    ("intlarhk;", "⨗"),
    ("intprod;", "⨼"),
    ("iocy;", "ё"),
    ("iogon;", "į"),
    ("iopf;", "𝕚"),
    ("iota;", "ι"),
    ("iprod;", "⨼"),
    ("iquest;", "¿"),
    ("iscr;", "𝒾"),
    ("isin;", "∈"),
    ("isinE;", "⋹"),
    ("isindot;", "⋵"),
    ("isins;", "⋴"),
    ("isinsv;", "⋳"),
    ("isinv;", "∈"),
    ("it;", "\u{2062}"),
    ("itilde;", "ĩ"),
    ("iukcy;", "і"),
    ("iuml;", "ï"),
    ("jcirc;", "ĵ"),
    ("jcy;", "й"),
    ("jfr;", "𝔧"),
    ("jmath;", "ȷ"),
    ("jopf;", "𝕛"),
    ("jscr;", "𝒿"),
    ("jsercy;", "ј"),
    ("jukcy;", "є"),
    ("kappa;", "κ"),
    ("kappav;", "ϰ"),
    ("kcedil;", "ķ"),
    ("kcy;", "к"),
    ("kfr;", "𝔨"),
    ("kgreen;", "ĸ"),
    ("khcy;", "х"),
    ("kjcy;", "ќ"),
    ("kopf;", "𝕜"),
    ("kscr;", "𝓀"),
    ("lAarr;", "⇚"),
    ("lArr;", "⇐"),
    ("lAtail;", "⤛"),
    ("lBarr;", "⤎"),
    ("lE;", "≦"),
    ("lEg;", "⪋"),
    ("lHar;", "⥢"),
    ("lacute;", "ĺ"),
    ("laemptyv;", "⦴"),
    ("lagran;", "ℒ"),
    ("lambda;", "λ"),
    ("lang;", "⟨"),
    ("langd;", "⦑"),
    ("langle;", "⟨"),
    ("lap;", "⪅"),
    ("laquo;", "«"),
    ("larr;", "←"),
    ("larrb;", "⇤"),
    ("larrbfs;", "⤟"),
    ("larrfs;", "⤝"),
    ("larrhk;", "↩"),
    ("larrlp;", "↫"),
    ("larrpl;", "⤹"),
    ("larrsim;", "⥳"),
    ("larrtl;", "↢"),
    ("lat;", "⪫"),
    ("latail;", "⤙"),
    ("late;", "⪭"),
    ("lates;", "⪭\u{fe00}"),
    ("lbarr;", "⤌"),
    ("lbbrk;", "❲"),
    ("lbrace;", "{"),
    ("lbrack;", "["),
    ("lbrke;", "⦋"),
    ("lbrksld;", "⦏"),
    ("lbrkslu;", "⦍"),
    ("lcaron;", "ľ"),
    ("lcedil;", "ļ"),
    ("lceil;", "⌈"),
    ("lcub;", "{"),
    ("lcy;", "л"),
    ("ldca;", "⤶"),
    ("ldquo;", "“"),
    ("ldquor;", "„"),
    ("ldrdhar;", "⥧"),
    ("ldrushar;", "⥋"),
    ("ldsh;", "↲"),
    ("le;", "≤"),
    ("leftarrow;", "←"),
    ("leftarrowtail;", "↢"),
    ("leftharpoondown;", "↽"),
    ("leftharpoonup;", "↼"),
    ("leftleftarrows;", "⇇"),
    ("leftrightarrow;", "↔"),
    ("leftrightarrows;", "⇆"),
    ("leftrightharpoons;", "⇋"),
    ("leftrightsquigarrow;", "↭"),
    ("leftthreetimes;", "⋋"),
    ("leg;", "⋚"),
    ("leq;", "≤"),
    ("leqq;", "≦"),
    ("leqslant;", "⩽"),
    ("les;", "⩽"),
    ("lescc;", "⪨"),
    ("lesdot;", "⩿"),
    ("lesdoto;", "⪁"),
    ("lesdotor;", "⪃"),
    ("lesg;", "⋚\u{fe00}"),
    ("lesges;", "⪓"),
    ("lessapprox;", "⪅"),
    ("lessdot;", "⋖"),
    ("lesseqgtr;", "⋚"),
    ("lesseqqgtr;", "⪋"),
    ("lessgtr;", "≶"),
    ("lesssim;", "≲"),
    ("lfisht;", "⥼"),
    ("lfloor;", "⌊"),
    ("lfr;", "𝔩"),
    ("lg;", "≶"),
    ("lgE;", "⪑"),
    ("lhard;", "↽"),
    ("lharu;", "↼"),
    ("lharul;", "⥪"),
    ("lhblk;", "▄"),
    ("ljcy;", "љ"),
    ("ll;", "≪"),
    ("llarr;", "⇇"),
    ("llcorner;", "⌞"),
    ("llhard;", "⥫"),
    ("lltri;", "◺"),
    ("lmidot;", "ŀ"),
    ("lmoust;", "⎰"),
    ("lmoustache;", "⎰"),
    ("lnE;", "≨"),
    ("lnap;", "⪉"),
    ("lnapprox;", "⪉"),
    ("lne;", "⪇"),
    ("lneq;", "⪇"),
    ("lneqq;", "≨"),
    ("lnsim;", "⋦"),
    ("loang;", "⟬"),
    ("loarr;", "⇽"),
    ("lobrk;", "⟦"),
    ("longleftarrow;", "⟵"),
    ("longleftrightarrow;", "⟷"),
    ("longmapsto;", "⟼"),
    ("longrightarrow;", "⟶"),
    ("looparrowleft;", "↫"),
    ("looparrowright;", "↬"),
    ("lopar;", "⦅"),
    ("lopf;", "𝕝"),
    ("loplus;", "⨭"),
    ("lotimes;", "⨴"),
    ("lowast;", "∗"),
    ("lowbar;", "_"),
    ("loz;", "◊"),
    ("lozenge;", "◊"),
    ("lozf;", "⧫"),
    ("lpar;", "("),
    ("lparlt;", "⦓"),
    ("lrarr;", "⇆"),
    ("lrcorner;", "⌟"),
    ("lrhar;", "⇋"),
    ("lrhard;", "⥭"),
    ("lrm;", "\u{200e}"),
    ("lrtri;", "⊿"),
    ("lsaquo;", "‹"),
    ("lscr;", "𝓁"),
    ("lsh;", "↰"),
    ("lsim;", "≲"),
    ("lsime;", "⪍"),
    ("lsimg;", "⪏"),
    ("lsqb;", "["),
    ("lsquo;", "‘"),
    ("lsquor;", "‚"),
    ("lstrok;", "ł"),
    ("lt;", "<"),
    ("ltcc;", "⪦"),
    ("ltcir;", "⩹"),
    ("ltdot;", "⋖"),
    ("lthree;", "⋋"),
    ("ltimes;", "⋉"),
    ("ltlarr;", "⥶"),
    ("ltquest;", "⩻"),
    ("ltrPar;", "⦖"),
    ("ltri;", "◃"),
    ("ltrie;", "⊴"),
    ("ltrif;", "◂"),
    ("lurdshar;", "⥊"),
    ("luruhar;", "⥦"),
    ("lvertneqq;", "≨\u{fe00}"),
    ("lvnE;", "≨\u{fe00}"),
    ("mDDot;", "∺"),
    ("macr;", "¯"),
    ("male;", "♂"),
    ("malt;", "✠"),
    ("maltese;", "✠"),
    ("map;", "↦"),
    ("mapsto;", "↦"),
    ("mapstodown;", "↧"),
    ("mapstoleft;", "↤"),
    ("mapstoup;", "↥"),
    ("marker;", "▮"),
    ("mcomma;", "⨩"),
    ("mcy;", "м"),
    ("mdash;", "—"),
    ("measuredangle;", "∡"),
    ("mfr;", "𝔪"),
    ("mho;", "℧"),
    ("micro;", "µ"),
    ("mid;", "∣"),
    ("midast;", "*"),
    ("midcir;", "⫰"),
    ("middot;", "·"),
    ("minus;", "−"),
    ("minusb;", "⊟"),
    ("minusd;", "∸"),
    ("minusdu;", "⨪"),
    ("mlcp;", "⫛"),
    ("mldr;", "…"),
    ("mnplus;", "∓"),
    ("models;", "⊧"),
    ("mopf;", "𝕞"),
    ("mp;", "∓"),
    ("mscr;", "𝓂"),
    ("mstpos;", "∾"),
    ("mu;", "μ"),
    ("multimap;", "⊸"),
    ("mumap;", "⊸"),
    ("nGg;", "⋙\u{338}"),
    ("nGt;", "≫\u{20d2}"),
    ("nGtv;", "≫\u{338}"),
    ("nLeftarrow;", "⇍"),
    ("nLeftrightarrow;", "⇎"),
    ("nLl;", "⋘\u{338}"),
    ("nLt;", "≪\u{20d2}"),
    ("nLtv;", "≪\u{338}"),
    ("nRightarrow;", "⇏"),
    ("nVDash;", "⊯"),
    ("nVdash;", "⊮"),
    ("nabla;", "∇"),
    ("nacute;", "ń"),
    ("nang;", "∠\u{20d2}"),
    ("nap;", "≉"),
    ("napE;", "⩰\u{338}"),
    ("napid;", "≋\u{338}"),
    ("napos;", "ŉ"),
    ("napprox;", "≉"),
    ("natur;", "♮"),
    ("natural;", "♮"),
    ("naturals;", "ℕ"),
    ("nbsp;", "\u{a0}"),
    ("nbump;", "≎\u{338}"),
    ("nbumpe;", "≏\u{338}"),
    ("ncap;", "⩃"),
    ("ncaron;", "ň"),
    ("ncedil;", "ņ"),
    ("ncong;", "≇"),
    ("ncongdot;", "⩭\u{338}"),
    ("ncup;", "⩂"),
    ("ncy;", "н"),
    ("ndash;", "–"),
    ("ne;", "≠"),
    ("neArr;", "⇗"),
    ("nearhk;", "⤤"),
    ("nearr;", "↗"),
    ("nearrow;", "↗"),
    ("nedot;", "≐\u{338}"),
    ("nequiv;", "≢"),
    ("nesear;", "⤨"),
    ("nesim;", "≂\u{338}"),
    ("nexist;", "∄"),
    ("nexists;", "∄"),
    ("nfr;", "𝔫"),
    ("ngE;", "≧\u{338}"),
    ("nge;", "≱"),
    ("ngeq;", "≱"),
    ("ngeqq;", "≧\u{338}"),
    ("ngeqslant;", "⩾\u{338}"),
    ("nges;", "⩾\u{338}"),
    ("ngsim;", "≵"),
    ("ngt;", "≯"),
    ("ngtr;", "≯"),
    ("nhArr;", "⇎"),
    ("nharr;", "↮"),
    ("nhpar;", "⫲"),
    ("ni;", "∋"),
    ("nis;", "⋼"),
    ("nisd;", "⋺"),
    ("niv;", "∋"),
    ("njcy;", "њ"),
    ("nlArr;", "⇍"),
    ("nlE;", "≦\u{338}"),
    ("nlarr;", "↚"),
    ("nldr;", "‥"),
    ("nle;", "≰"),
    ("nleftarrow;", "↚"),
    ("nleftrightarrow;", "↮"),
    ("nleq;", "≰"),
    ("nleqq;", "≦\u{338}"),
    ("nleqslant;", "⩽\u{338}"),
    ("nles;", "⩽\u{338}"),
    ("nless;", "≮"),
    ("nlsim;", "≴"),
    ("nlt;", "≮"),
    ("nltri;", "⋪"),
    ("nltrie;", "⋬"),
    ("nmid;", "∤"),
    ("nopf;", "𝕟"),
    ("not;", "¬"),
    ("notin;", "∉"),
    ("notinE;", "⋹\u{338}"),
    ("notindot;", "⋵\u{338}"),
    ("notinva;", "∉"),
    ("notinvb;", "⋷"),
    ("notinvc;", "⋶"),
    ("notni;", "∌"),
    ("notniva;", "∌"),
    ("notnivb;", "⋾"),
    ("notnivc;", "⋽"),
    ("npar;", "∦"),
    ("nparallel;", "∦"),
    ("nparsl;", "⫽\u{20e5}"),
    ("npart;", "∂\u{338}"),
    ("npolint;", "⨔"),
    ("npr;", "⊀"),
    ("nprcue;", "⋠"),
    ("npre;", "⪯\u{338}"),
    ("nprec;", "⊀"),
    ("npreceq;", "⪯\u{338}"),
    ("nrArr;", "⇏"),
    ("nrarr;", "↛"),
    ("nrarrc;", "⤳\u{338}"),
    ("nrarrw;", "↝\u{338}"),
    ("nrightarrow;", "↛"),
    ("nrtri;", "⋫"),
    ("nrtrie;", "⋭"),
    ("nsc;", "⊁"),
    ("nsccue;", "⋡"),
    ("nsce;", "⪰\u{338}"),
    ("nscr;", "𝓃"),
    ("nshortmid;", "∤"),
    ("nshortparallel;", "∦"),
    ("nsim;", "≁"),
    ("nsime;", "≄"),
    ("nsimeq;", "≄"),
    ("nsmid;", "∤"),
    ("nspar;", "∦"),
    ("nsqsube;", "⋢"),
    ("nsqsupe;", "⋣"),
    ("nsub;", "⊄"),
    ("nsubE;", "⫅\u{338}"),
    ("nsube;", "⊈"),
    ("nsubset;", "⊂\u{20d2}"),
    ("nsubseteq;", "⊈"),
    ("nsubseteqq;", "⫅\u{338}"),
    ("nsucc;", "⊁"),
    ("nsucceq;", "⪰\u{338}"),
    ("nsup;", "⊅"),
    ("nsupE;", "⫆\u{338}"),
    ("nsupe;", "⊉"),
    ("nsupset;", "⊃\u{20d2}"),
    ("nsupseteq;", "⊉"),
    ("nsupseteqq;", "⫆\u{338}"),
    ("ntgl;", "≹"),
    ("ntilde;", "ñ"),
    ("ntlg;", "≸"),
    ("ntriangleleft;", "⋪"),
    ("ntrianglelefteq;", "⋬"),
    ("ntriangleright;", "⋫"),
    ("ntrianglerighteq;", "⋭"),
    ("nu;", "ν"),
    ("num;", "#"),
    ("numero;", "№"),
    ("numsp;", "\u{2007}"),
    ("nvDash;", "⊭"),
    ("nvHarr;", "⤄"),
    ("nvap;", "≍\u{20d2}"),
    ("nvdash;", "⊬"),
    ("nvge;", "≥\u{20d2}"),
    ("nvgt;", ">\u{20d2}"),
    ("nvinfin;", "⧞"),
    ("nvlArr;", "⤂"),
    ("nvle;", "≤\u{20d2}"),
    ("nvlt;", "<\u{20d2}"),
    ("nvltrie;", "⊴\u{20d2}"),
    ("nvrArr;", "⤃"),
    ("nvrtrie;", "⊵\u{20d2}"),
    ("nvsim;", "∼\u{20d2}"),
    ("nwArr;", "⇖"),
    ("nwarhk;", "⤣"),
    ("nwarr;", "↖"),
    ("nwarrow;", "↖"),
    ("nwnear;", "⤧"),
    ("oS;", "Ⓢ"),
    ("oacute;", "ó"),
    ("oast;", "⊛"),
    ("ocir;", "⊚"),
    ("ocirc;", "ô"),
    ("ocy;", "о"),
    ("odash;", "⊝"),
    ("odblac;", "ő"),
    ("odiv;", "⨸"),
    ("odot;", "⊙"),
    ("odsold;", "⦼"),
    ("oelig;", "œ"),
    ("ofcir;", "⦿"),
    ("ofr;", "𝔬"),
    ("ogon;", "˛"),
    ("ograve;", "ò"),
    ("ogt;", "⧁"),
    ("ohbar;", "⦵"),
    ("ohm;", "Ω"),
    ("oint;", "∮"),
    ("olarr;", "↺"),
    ("olcir;", "⦾"),
    ("olcross;", "⦻"),
    ("oline;", "‾"),
    ("olt;", "⧀"),
    ("omacr;", "ō"),
    ("omega;", "ω"),
    ("omicron;", "ο"),
    ("omid;", "⦶"),
    ("ominus;", "⊖"),
    ("oopf;", "𝕠"),
    ("opar;", "⦷"),
    ("operp;", "⦹"),
    ("oplus;", "⊕"),
    ("or;", "∨"),
    ("orarr;", "↻"),
    ("ord;", "⩝"),
    ("order;", "ℴ"),
    ("orderof;", "ℴ"),
    ("ordf;", "ª"),
    ("ordm;", "º"),
    ("origof;", "⊶"),
    ("oror;", "⩖"),
    ("orslope;", "⩗"),
    ("orv;", "⩛"),
    ("oscr;", "ℴ"),
    ("oslash;", "ø"),
    ("osol;", "⊘"),
    ("otilde;", "õ"),
    ("otimes;", "⊗"),
    ("otimesas;", "⨶"),
    ("ouml;", "ö"),
    ("ovbar;", "⌽"),
    ("par;", "∥"),
    ("para;", "¶"),
    ("parallel;", "∥"),
    ("parsim;", "⫳"),
    ("parsl;", "⫽"),
    ("part;", "∂"),
    ("pcy;", "п"),
    ("percnt;", "%"),
    ("period;", "."),
    ("permil;", "‰"),
    ("perp;", "⊥"),
    ("pertenk;", "‱"),
    ("pfr;", "𝔭"),
    ("phi;", "φ"),
    ("phiv;", "ϕ"),
    ("phmmat;", "ℳ"),
    ("phone;", "☎"),
    ("pi;", "π"),
    ("pitchfork;", "⋔"),
    ("piv;", "ϖ"),
    ("planck;", "ℏ"),
    ("planckh;", "ℎ"),
    ("plankv;", "ℏ"),
    ("plus;", "+"),
    ("plusacir;", "⨣"),
    ("plusb;", "⊞"),
    ("pluscir;", "⨢"),
    ("plusdo;", "∔"),
    ("plusdu;", "⨥"),
    ("pluse;", "⩲"),
    ("plusmn;", "±"),
    ("plussim;", "⨦"),
    ("plustwo;", "⨧"),
    ("pm;", "±"),
    ("pointint;", "⨕"),
    ("popf;", "𝕡"),
    ("pound;", "£"),
    ("pr;", "≺"),
    ("prE;", "⪳"),
    ("prap;", "⪷"),
    ("prcue;", "≼"),
    ("pre;", "⪯"),
    ("prec;", "≺"),
    ("precapprox;", "⪷"),
    ("preccurlyeq;", "≼"),
    ("preceq;", "⪯"),
    ("precnapprox;", "⪹"),
    ("precneqq;", "⪵"),
    ("precnsim;", "⋨"),
    ("precsim;", "≾"),
    ("prime;", "′"),
    ("primes;", "ℙ"),
    ("prnE;", "⪵"),
    ("prnap;", "⪹"),
    ("prnsim;", "⋨"),
    ("prod;", "∏"),
    ("profalar;", "⌮"),
    ("profline;", "⌒"),
    ("profsurf;", "⌓"),
    ("prop;", "∝"),
    ("propto;", "∝"),
    ("prsim;", "≾"),
    ("prurel;", "⊰"),
    ("pscr;", "𝓅"),
    ("psi;", "ψ"),
    ("puncsp;", "\u{2008}"),
    ("qfr;", "𝔮"),
    ("qint;", "⨌"),
    ("qopf;", "𝕢"),
    ("qprime;", "⁗"),
    ("qscr;", "𝓆"),
    ("quaternions;", "ℍ"),
    ("quatint;", "⨖"),
    ("quest;", "?"),
    ("questeq;", "≟"),
    ("quot;", "\""),
    ("rAarr;", "⇛"),
    ("rArr;", "⇒"),
    ("rAtail;", "⤜"),
    ("rBarr;", "⤏"),
    ("rHar;", "⥤"),
    ("race;", "∽\u{331}"),
    ("racute;", "ŕ"),
    ("radic;", "√"),
    ("raemptyv;", "⦳"),
    ("rang;", "⟩"),
    ("rangd;", "⦒"),
    ("range;", "⦥"),
    ("rangle;", "⟩"),
    ("raquo;", "»"),
    ("rarr;", "→"),
    ("rarrap;", "⥵"),
    ("rarrb;", "⇥"),
    ("rarrbfs;", "⤠"),
    ("rarrc;", "⤳"),
    ("rarrfs;", "⤞"),
    ("rarrhk;", "↪"),
    ("rarrlp;", "↬"),
    ("rarrpl;", "⥅"),
    ("rarrsim;", "⥴"),
    ("rarrtl;", "↣"),
    ("rarrw;", "↝"),
    ("ratail;", "⤚"),
    ("ratio;", "∶"),
    ("rationals;", "ℚ"),
    ("rbarr;", "⤍"),
    ("rbbrk;", "❳"),
    ("rbrace;", "}"),
    ("rbrack;", "]"),
    ("rbrke;", "⦌"),
    ("rbrksld;", "⦎"),
    ("rbrkslu;", "⦐"),
    ("rcaron;", "ř"),
    ("rcedil;", "ŗ"),
    ("rceil;", "⌉"),
    ("rcub;", "}"),
    ("rcy;", "р"),
    ("rdca;", "⤷"),
    ("rdldhar;", "⥩"),
    ("rdquo;", "”"),
    ("rdquor;", "”"),
    ("rdsh;", "↳"),
    ("real;", "ℜ"),
    ("realine;", "ℛ"),
    ("realpart;", "ℜ"),
    ("reals;", "ℝ"),
    ("rect;", "▭"),
    ("reg;", "®"),
    ("rfisht;", "⥽"),
    ("rfloor;", "⌋"),
    ("rfr;", "𝔯"),
    ("rhard;", "⇁"),
    ("rharu;", "⇀"),
    ("rharul;", "⥬"),
    ("rho;", "ρ"),
    ("rhov;", "ϱ"),
    ("rightarrow;", "→"),
    ("rightarrowtail;", "↣"),
    ("rightharpoondown;", "⇁"),
    ("rightharpoonup;", "⇀"),
    ("rightleftarrows;", "⇄"),
    ("rightleftharpoons;", "⇌"),
    ("rightrightarrows;", "⇉"),
    ("rightsquigarrow;", "↝"),
    ("rightthreetimes;", "⋌"),
    ("ring;", "˚"),
    ("risingdotseq;", "≓"),
    ("rlarr;", "⇄"),
    ("rlhar;", "⇌"),
    ("rlm;", "\u{200f}"),
    ("rmoust;", "⎱"),
    ("rmoustache;", "⎱"),
    ("rnmid;", "⫮"),
    ("roang;", "⟭"),
    ("roarr;", "⇾"),
    ("robrk;", "⟧"),
    ("ropar;", "⦆"),
    ("ropf;", "𝕣"),
    ("roplus;", "⨮"),
    ("rotimes;", "⨵"),
    ("rpar;", ")"),
    ("rpargt;", "⦔"),
    ("rppolint;", "⨒"),
    ("rrarr;", "⇉"),
    ("rsaquo;", "›"),
    ("rscr;", "𝓇"),
    ("rsh;", "↱"),
    ("rsqb;", "]"),
    ("rsquo;", "’"),
    ("rsquor;", "’"),
    ("rthree;", "⋌"),
    ("rtimes;", "⋊"),
    ("rtri;", "▹"),
    ("rtrie;", "⊵"),
    ("rtrif;", "▸"),
    ("rtriltri;", "⧎"),
    ("ruluhar;", "⥨"),
    ("rx;", "℞"),
    ("sacute;", "ś"),
    ("sbquo;", "‚"),
    ("sc;", "≻"),
    ("scE;", "⪴"),
    ("scap;", "⪸"),
    ("scaron;", "š"),
    ("sccue;", "≽"),
    ("sce;", "⪰"),
    ("scedil;", "ş"),
    ("scirc;", "ŝ"),
    ("scnE;", "⪶"),
    ("scnap;", "⪺"),
    ("scnsim;", "⋩"),
    ("scpolint;", "⨓"),
    ("scsim;", "≿"),
    ("scy;", "с"),
    ("sdot;", "⋅"),
    ("sdotb;", "⊡"),
    ("sdote;", "⩦"),
    ("seArr;", "⇘"),
    ("searhk;", "⤥"),
    ("searr;", "↘"),
    ("searrow;", "↘"),
    ("sect;", "§"),
    ("semi;", ";"),
    ("seswar;", "⤩"),
    ("setminus;", "∖"),
    ("setmn;", "∖"),
    ("sext;", "✶"),
    ("sfr;", "𝔰"),
    ("sfrown;", "⌢"),
    ("sharp;", "♯"),
    ("shchcy;", "щ"),
    ("shcy;", "ш"),
    ("shortmid;", "∣"),
    ("shortparallel;", "∥"),
    ("shy;", "\u{ad}"),
    ("sigma;", "σ"),
    ("sigmaf;", "ς"),
    ("sigmav;", "ς"),
    ("sim;", "∼"),
    ("simdot;", "⩪"),
    ("sime;", "≃"),
    ("simeq;", "≃"),
    ("simg;", "⪞"),
    ("simgE;", "⪠"),
    ("siml;", "⪝"),
    ("simlE;", "⪟"),
    ("simne;", "≆"),
    ("simplus;", "⨤"),
    ("simrarr;", "⥲"),
    ("slarr;", "←"),
    ("smallsetminus;", "∖"),
    ("smashp;", "⨳"),
    ("smeparsl;", "⧤"),
    ("smid;", "∣"),
    ("smile;", "⌣"),
    ("smt;", "⪪"),
    ("smte;", "⪬"),
    ("smtes;", "⪬\u{fe00}"),
    ("softcy;", "ь"),
    ("sol;", "/"),
    ("solb;", "⧄"),
    ("solbar;", "⌿"),
    ("sopf;", "𝕤"),
    ("spades;", "♠"),
    ("spadesuit;", "♠"),
    ("spar;", "∥"),
    ("sqcap;", "⊓"),
    ("sqcaps;", "⊓\u{fe00}"),
    ("sqcup;", "⊔"),
    ("sqcups;", "⊔\u{fe00}"),
    ("sqsub;", "⊏"),
    ("sqsube;", "⊑"),
    ("sqsubset;", "⊏"),
    ("sqsubseteq;", "⊑"),
    ("sqsup;", "⊐"),
    ("sqsupe;", "⊒"),
    ("sqsupset;", "⊐"),
    ("sqsupseteq;", "⊒"),
    ("squ;", "□"),
    ("square;", "□"),
    ("squarf;", "▪"),
    ("squf;", "▪"),
    ("srarr;", "→"),
    ("sscr;", "𝓈"),
    ("ssetmn;", "∖"),
    ("ssmile;", "⌣"),
    ("sstarf;", "⋆"),
    ("star;", "☆"),
    ("starf;", "★"),
    ("straightepsilon;", "ϵ"),
    ("straightphi;", "ϕ"),
    ("strns;", "¯"),
    ("sub;", "⊂"),
    ("subE;", "⫅"),
    ("subdot;", "⪽"),
    ("sube;", "⊆"),
    ("subedot;", "⫃"),
    ("submult;", "⫁"),
    ("subnE;", "⫋"),
    ("subne;", "⊊"),
    ("subplus;", "⪿"),
    ("subrarr;", "⥹"),
    ("subset;", "⊂"),
    ("subseteq;", "⊆"),
    ("subseteqq;", "⫅"),
    ("subsetneq;", "⊊"),
    ("subsetneqq;", "⫋"),
    ("subsim;", "⫇"),
    ("subsub;", "⫕"),
    ("subsup;", "⫓"),
    ("succ;", "≻"),
    ("succapprox;", "⪸"),
    ("succcurlyeq;", "≽"),
    ("succeq;", "⪰"),
    ("succnapprox;", "⪺"),
    ("succneqq;", "⪶"),
    ("succnsim;", "⋩"),
    ("succsim;", "≿"),
    ("sum;", "∑"),
    ("sung;", "♪"),
    ("sup1;", "¹"),
    ("sup2;", "²"),
    ("sup3;", "³"),
    ("sup;", "⊃"),
    ("supE;", "⫆"),
    ("supdot;", "⪾"),
    ("supdsub;", "⫘"),
    ("supe;", "⊇"),
    ("supedot;", "⫄"),
    ("suphsol;", "⟉"),
    ("suphsub;", "⫗"),
    ("suplarr;", "⥻"),
    ("supmult;", "⫂"),
    ("supnE;", "⫌"),
    ("supne;", "⊋"),
    ("supplus;", "⫀"),
    ("supset;", "⊃"),
    ("supseteq;", "⊇"),
    ("supseteqq;", "⫆"),
    ("supsetneq;", "⊋"),
    ("supsetneqq;", "⫌"),
    ("supsim;", "⫈"),
    ("supsub;", "⫔"),
    ("supsup;", "⫖"),
    ("swArr;", "⇙"),
    ("swarhk;", "⤦"),
    ("swarr;", "↙"),
    ("swarrow;", "↙"),
    ("swnwar;", "⤪"),
    ("szlig;", "ß"),
    ("target;", "⌖"),
    ("tau;", "τ"),
    ("tbrk;", "⎴"),
    ("tcaron;", "ť"),
    ("tcedil;", "ţ"),
    ("tcy;", "т"),
    ("tdot;", "\u{20db}"),
    ("telrec;", "⌕"),
    ("tfr;", "𝔱"),
    ("there4;", "∴"),
    ("therefore;", "∴"),
    ("theta;", "θ"),
    ("thetasym;", "ϑ"),
    ("thetav;", "ϑ"),
    ("thickapprox;", "≈"),
    ("thicksim;", "∼"),
    ("thinsp;", "\u{2009}"),
    ("thkap;", "≈"),
    ("thksim;", "∼"),
    ("thorn;", "þ"),
    ("tilde;", "˜"),
    ("times;", "×"),
    ("timesb;", "⊠"),
    ("timesbar;", "⨱"),
    ("timesd;", "⨰"),
    ("tint;", "∭"),
    ("toea;", "⤨"),
    ("top;", "⊤"),
    ("topbot;", "⌶"),
    ("topcir;", "⫱"),
    ("topf;", "𝕥"),
    ("topfork;", "⫚"),
    ("tosa;", "⤩"),
    ("tprime;", "‴"),
    ("trade;", "™"),
    ("triangle;", "▵"),
    ("triangledown;", "▿"),
    ("triangleleft;", "◃"),
    ("trianglelefteq;", "⊴"),
    ("triangleq;", "≜"),
    ("triangleright;", "▹"),
    ("trianglerighteq;", "⊵"),
    ("tridot;", "◬"),
    ("trie;", "≜"),
    ("triminus;", "⨺"),
    ("triplus;", "⨹"),
    ("trisb;", "⧍"),
    ("tritime;", "⨻"),
    ("trpezium;", "⏢"),
    ("tscr;", "𝓉"),
    ("tscy;", "ц"),
    ("tshcy;", "ћ"),
    ("tstrok;", "ŧ"),
    ("twixt;", "≬"),
    ("twoheadleftarrow;", "↞"),
    ("twoheadrightarrow;", "↠"),
    ("uArr;", "⇑"),
    ("uHar;", "⥣"),
    ("uacute;", "ú"),
    ("uarr;", "↑"),
    ("ubrcy;", "ў"),
    ("ubreve;", "ŭ"),
    ("ucirc;", "û"),
    ("ucy;", "у"),
    ("udarr;", "⇅"),
    ("udblac;", "ű"),
    ("udhar;", "⥮"),
    ("ufisht;", "⥾"),
    ("ufr;", "𝔲"),
    ("ugrave;", "ù"),
    ("uharl;", "↿"),
    ("uharr;", "↾"),
    ("uhblk;", "▀"),
    ("ulcorn;", "⌜"),
    ("ulcorner;", "⌜"),
    ("ulcrop;", "⌏"),
    ("ultri;", "◸"),
    ("umacr;", "ū"),
    ("uml;", "¨"),
    ("uogon;", "ų"),
    ("uopf;", "𝕦"),
    ("uparrow;", "↑"),
    ("updownarrow;", "↕"),
    ("upharpoonleft;", "↿"),
    ("upharpoonright;", "↾"),
    ("uplus;", "⊎"),
    ("upsi;", "υ"),
    ("upsih;", "ϒ"),
    ("upsilon;", "υ"),
    ("upuparrows;", "⇈"),
    ("urcorn;", "⌝"),
    ("urcorner;", "⌝"),
    ("urcrop;", "⌎"),
    ("uring;", "ů"),
    ("urtri;", "◹"),
    ("uscr;", "𝓊"),
    ("utdot;", "⋰"),
    ("utilde;", "ũ"),
    ("utri;", "▵"),
    ("utrif;", "▴"),
    ("uuarr;", "⇈"),
    ("uuml;", "ü"),
    ("uwangle;", "⦧"),
    ("vArr;", "⇕"),
    ("vBar;", "⫨"),
    ("vBarv;", "⫩"),
    ("vDash;", "⊨"),
    ("vangrt;", "⦜"),
    ("varepsilon;", "ϵ"),
    ("varkappa;", "ϰ"),
    ("varnothing;", "∅"),
    ("varphi;", "ϕ"),
    ("varpi;", "ϖ"),
    ("varpropto;", "∝"),
    ("varr;", "↕"),
    ("varrho;", "ϱ"),
    ("varsigma;", "ς"),
    ("varsubsetneq;", "⊊\u{fe00}"),
    ("varsubsetneqq;", "⫋\u{fe00}"),
    ("varsupsetneq;", "⊋\u{fe00}"),
    ("varsupsetneqq;", "⫌\u{fe00}"),
    ("vartheta;", "ϑ"),
    ("vartriangleleft;", "⊲"),
    ("vartriangleright;", "⊳"),
    ("vcy;", "в"),
    ("vdash;", "⊢"),
    ("vee;", "∨"),
    ("veebar;", "⊻"),
    ("veeeq;", "≚"),
    ("vellip;", "⋮"),
    ("verbar;", "|"),
    ("vert;", "|"),
    ("vfr;", "𝔳"),
    ("vltri;", "⊲"),
    ("vnsub;", "⊂\u{20d2}"),
    ("vnsup;", "⊃\u{20d2}"),
    ("vopf;", "𝕧"),
    ("vprop;", "∝"),
    ("vrtri;", "⊳"),
    ("vscr;", "𝓋"),
    ("vsubnE;", "⫋\u{fe00}"),
    ("vsubne;", "⊊\u{fe00}"),
    ("vsupnE;", "⫌\u{fe00}"),
    ("vsupne;", "⊋\u{fe00}"),
    ("vzigzag;", "⦚"),
    ("wcirc;", "ŵ"),
    ("wedbar;", "⩟"),
    ("wedge;", "∧"),
    ("wedgeq;", "≙"),
    ("weierp;", "℘"),
    ("wfr;", "𝔴"),
    ("wopf;", "𝕨"),
    ("wp;", "℘"),
    ("wr;", "≀"),
    ("wreath;", "≀"),
    ("wscr;", "𝓌"),
    ("xcap;", "⋂"),
    ("xcirc;", "◯"),
    ("xcup;", "⋃"),
    ("xdtri;", "▽"),
    ("xfr;", "𝔵"),
    ("xhArr;", "⟺"),
    ("xharr;", "⟷"),
    ("xi;", "ξ"),
    ("xlArr;", "⟸"),
    ("xlarr;", "⟵"),
    ("xmap;", "⟼"),
    ("xnis;", "⋻"),
    ("xodot;", "⨀"),
    ("xopf;", "𝕩"),
    ("xoplus;", "⨁"),
    ("xotime;", "⨂"),
    ("xrArr;", "⟹"),
    ("xrarr;", "⟶"),
    ("xscr;", "𝓍"),
    ("xsqcup;", "⨆"),
    ("xuplus;", "⨄"),
    ("xutri;", "△"),
    ("xvee;", "⋁"),
    ("xwedge;", "⋀"),
    ("yacute;", "ý"),
    ("yacy;", "я"),
    ("ycirc;", "ŷ"),
    ("ycy;", "ы"),
    ("yen;", "¥"),
    ("yfr;", "𝔶"),
    ("yicy;", "ї"),
    ("yopf;", "𝕪"),
    ("yscr;", "𝓎"),
    ("yucy;", "ю"),
    ("yuml;", "ÿ"),
    ("zacute;", "ź"),
    ("zcaron;", "ž"),
    ("zcy;", "з"),
    ("zdot;", "ż"),
    ("zeetrf;", "ℨ"),
    ("zeta;", "ζ"),
    ("zfr;", "𝔷"),
    ("zhcy;", "ж"),
    ("zigrarr;", "⇝"),
    ("zopf;", "𝕫"),
    ("zscr;", "𝓏"),
    ("zwj;", "\u{200d}"),
    ("zwnj;", "\u{200c}"),
];

const LEGACY_ENTITIES: &[&str] = &["AElig", "AMP", "Aacute", "Acirc", "Agrave", "Aring", "Atilde", "Auml", "COPY", "Ccedil", "ETH", "Eacute", "Ecirc", "Egrave", "Euml", "GT", "Iacute", "Icirc", "Igrave", "Iuml", "LT", "Ntilde", "Oacute", "Ocirc", "Ograve", "Oslash", "Otilde", "Ouml", "QUOT", "REG", "THORN", "Uacute", "Ucirc", "Ugrave", "Uuml", "Yacute", "aacute", "acirc", "acute", "aelig", "agrave", "amp", "aring", "atilde", "auml", "brvbar", "ccedil", "cedil", "cent", "copy", "curren", "deg", "divide", "eacute", "ecirc", "egrave", "eth", "euml", "frac12", "frac14", "frac34", "gt", "iacute", "icirc", "iexcl", "igrave", "iquest", "iuml", "laquo", "lt", "macr", "micro", "middot", "nbsp", "not", "ntilde", "oacute", "ocirc", "ograve", "ordf", "ordm", "oslash", "otilde", "ouml", "para", "plusmn", "pound", "quot", "raquo", "reg", "sect", "shy", "sup1", "sup2", "sup3", "szlig", "thorn", "times", "uacute", "ucirc", "ugrave", "uml", "uuml", "yacute", "yen", "yuml"];

/// `__elephc_dom_entity_table` — transcribed from the PHP form.
fn decl_fn_elephc_dom_entity_table() -> Stmt {
    function("__elephc_dom_entity_table")
        .returns(t_array())
        .body(vec![
            s_return(e_array_assoc(
                ENTITY_TABLE.iter().map(|(name, text)| (e_str(name), e_str(text))).collect(),
            )),
        ])
        .build()
}

/// `__elephc_dom_legacy_entities` — transcribed from the PHP form.
fn decl_fn_elephc_dom_legacy_entities() -> Stmt {
    function("__elephc_dom_legacy_entities")
        .returns(t_array())
        .body(vec![
            s_return(e_array(LEGACY_ENTITIES.iter().map(|name| e_str(name)).collect())),
        ])
        .build()
}

/// Builds the whole surface, one declaration per helper above.
/// `__construct` lifted out of `decl_class_dom_nodelist` so it builds in its own stack frame.
fn decl_class_dom_nodelist_m1() -> MethodBuilder {
    method("__construct")
        .param_default("items", t_array(), e_array(vec![]))
        .body(vec![
            s_prop_assign(e_this(), "items", e_var("items")),
            s_prop_assign(e_this(), "length", e_call("count", vec![e_var("items")])),
        ])
}

/// `item` lifted out of `decl_class_dom_nodelist` so it builds in its own stack frame.
fn decl_class_dom_nodelist_m2() -> MethodBuilder {
    method("item")
        .param("index", TypeExpr::Int)
        .returns(t_nullable(t_class("\\Dom\\Node")))
        .body(vec![
            s_if(
                e_binop(e_binop(e_var("index"), BinOp::Lt, e_int(0)), BinOp::Or, e_binop(e_var("index"), BinOp::GtEq, e_this_prop("length"))),
                vec![
                    s_return(e_null()),
                ],
                vec![],
                None,
            ),
            s_return(e_index(e_this_prop("items"), e_var("index"))),
        ])
}

/// `count` lifted out of `decl_class_dom_nodelist` so it builds in its own stack frame.
fn decl_class_dom_nodelist_m3() -> MethodBuilder {
    method("count")
        .returns(TypeExpr::Int)
        .body(vec![
            s_return(e_this_prop("length")),
        ])
}

/// `getIterator` lifted out of `decl_class_dom_nodelist` so it builds in its own stack frame.
fn decl_class_dom_nodelist_m4() -> MethodBuilder {
    method("getIterator")
        .returns(t_class("\\Iterator"))
        .body(vec![
            s_return(e_new_fq("ArrayIterator", vec![e_this_prop("items")])),
        ])
}

/// `__construct` lifted out of `decl_class_dom_htmlcollection` so it builds in its own stack frame.
fn decl_class_dom_htmlcollection_m5() -> MethodBuilder {
    method("__construct")
        .param_default("items", t_array(), e_array(vec![]))
        .body(vec![
            s_prop_assign(e_this(), "items", e_var("items")),
            s_prop_assign(e_this(), "length", e_call("count", vec![e_var("items")])),
        ])
}

/// `item` lifted out of `decl_class_dom_htmlcollection` so it builds in its own stack frame.
fn decl_class_dom_htmlcollection_m6() -> MethodBuilder {
    method("item")
        .param("index", TypeExpr::Int)
        .returns(t_nullable(t_class("\\Dom\\Element")))
        .body(vec![
            s_if(
                e_binop(e_binop(e_var("index"), BinOp::Lt, e_int(0)), BinOp::Or, e_binop(e_var("index"), BinOp::GtEq, e_this_prop("length"))),
                vec![
                    s_return(e_null()),
                ],
                vec![],
                None,
            ),
            s_return(e_index(e_this_prop("items"), e_var("index"))),
        ])
}

/// `namedItem` lifted out of `decl_class_dom_htmlcollection` so it builds in its own stack frame.
fn decl_class_dom_htmlcollection_m7() -> MethodBuilder {
    method("namedItem")
        .param("key", TypeExpr::Str)
        .returns(t_nullable(t_class("\\Dom\\Element")))
        .body(vec![
            s_if(
                e_binop(e_var("key"), BinOp::StrictEq, e_str("")),
                vec![
                    s_return(e_null()),
                ],
                vec![],
                None,
            ),
            s_foreach(e_this_prop("items"), None, "element", vec![
                s_if(
                    e_binop(e_method_call(e_var("element"), "getAttribute", vec![e_str("id")]), BinOp::StrictEq, e_var("key")),
                    vec![
                        s_return(e_var("element")),
                    ],
                    vec![],
                    None,
                ),
            ]),
            s_foreach(e_this_prop("items"), None, "element", vec![
                s_if(
                    e_binop(e_method_call(e_var("element"), "getAttribute", vec![e_str("name")]), BinOp::StrictEq, e_var("key")),
                    vec![
                        s_return(e_var("element")),
                    ],
                    vec![],
                    None,
                ),
            ]),
            s_return(e_null()),
        ])
}

/// `count` lifted out of `decl_class_dom_htmlcollection` so it builds in its own stack frame.
fn decl_class_dom_htmlcollection_m8() -> MethodBuilder {
    method("count")
        .returns(TypeExpr::Int)
        .body(vec![
            s_return(e_this_prop("length")),
        ])
}

/// `getIterator` lifted out of `decl_class_dom_htmlcollection` so it builds in its own stack frame.
fn decl_class_dom_htmlcollection_m9() -> MethodBuilder {
    method("getIterator")
        .returns(t_class("\\Iterator"))
        .body(vec![
            s_return(e_new_fq("ArrayIterator", vec![e_this_prop("items")])),
        ])
}

/// `__construct` lifted out of `decl_class_dom_namednodemap` so it builds in its own stack frame.
fn decl_class_dom_namednodemap_m10() -> MethodBuilder {
    method("__construct")
        .param_default("items", t_array(), e_array(vec![]))
        .body(vec![
            s_prop_assign(e_this(), "items", e_var("items")),
            s_prop_assign(e_this(), "length", e_call("count", vec![e_var("items")])),
        ])
}

/// `item` lifted out of `decl_class_dom_namednodemap` so it builds in its own stack frame.
fn decl_class_dom_namednodemap_m11() -> MethodBuilder {
    method("item")
        .param("index", TypeExpr::Int)
        .returns(t_nullable(t_class("\\Dom\\Attr")))
        .body(vec![
            s_if(
                e_binop(e_binop(e_var("index"), BinOp::Lt, e_int(0)), BinOp::Or, e_binop(e_var("index"), BinOp::GtEq, e_this_prop("length"))),
                vec![
                    s_return(e_null()),
                ],
                vec![],
                None,
            ),
            s_return(e_index(e_this_prop("items"), e_var("index"))),
        ])
}

/// `getNamedItem` lifted out of `decl_class_dom_namednodemap` so it builds in its own stack frame.
fn decl_class_dom_namednodemap_m12() -> MethodBuilder {
    method("getNamedItem")
        .param("qualifiedName", TypeExpr::Str)
        .returns(t_nullable(t_class("\\Dom\\Attr")))
        .body(vec![
            s_foreach(e_this_prop("items"), None, "attr", vec![
                s_if(
                    e_binop(e_prop(e_var("attr"), "name"), BinOp::StrictEq, e_var("qualifiedName")),
                    vec![
                        s_return(e_var("attr")),
                    ],
                    vec![],
                    None,
                ),
            ]),
            s_assign("wanted", e_call("strtolower", vec![e_var("qualifiedName")])),
            s_foreach(e_this_prop("items"), None, "attr", vec![
                s_if(
                    e_binop(e_prop(e_var("attr"), "name"), BinOp::StrictEq, e_var("wanted")),
                    vec![
                        s_return(e_var("attr")),
                    ],
                    vec![],
                    None,
                ),
            ]),
            s_return(e_null()),
        ])
}

/// `count` lifted out of `decl_class_dom_namednodemap` so it builds in its own stack frame.
fn decl_class_dom_namednodemap_m13() -> MethodBuilder {
    method("count")
        .returns(TypeExpr::Int)
        .body(vec![
            s_return(e_this_prop("length")),
        ])
}

/// `getIterator` lifted out of `decl_class_dom_namednodemap` so it builds in its own stack frame.
fn decl_class_dom_namednodemap_m14() -> MethodBuilder {
    method("getIterator")
        .returns(t_class("\\Iterator"))
        .body(vec![
            s_return(e_new_fq("ArrayIterator", vec![e_this_prop("items")])),
        ])
}

/// `__construct` lifted out of `decl_class_dom_node` so it builds in its own stack frame.
fn decl_class_dom_node_m15() -> MethodBuilder {
    method("__construct")
        .body(vec![
            s_prop_assign(e_this(), "childNodes", e_new_fq("Dom\\NodeList", vec![e_array(vec![])])),
        ])
}

/// `hasChildNodes` lifted out of `decl_class_dom_node` so it builds in its own stack frame.
fn decl_class_dom_node_m16() -> MethodBuilder {
    method("hasChildNodes")
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_binop(e_call("count", vec![e_this_prop("kids")]), BinOp::Gt, e_int(0))),
        ])
}

/// `isSameNode` lifted out of `decl_class_dom_node` so it builds in its own stack frame.
fn decl_class_dom_node_m17() -> MethodBuilder {
    method("isSameNode")
        .param("otherNode", t_class("\\Dom\\Node"))
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_binop(e_this(), BinOp::StrictEq, e_var("otherNode"))),
        ])
}

/// `contains` lifted out of `decl_class_dom_node` so it builds in its own stack frame.
fn decl_class_dom_node_m18() -> MethodBuilder {
    method("contains")
        .param("other", t_nullable(t_class("\\Dom\\Node")))
        .returns(TypeExpr::Bool)
        .body(vec![
            s_assign("walk", e_var("other")),
            s_while(e_binop(e_var("walk"), BinOp::StrictNotEq, e_null()), vec![
                s_if(
                    e_binop(e_var("walk"), BinOp::StrictEq, e_this()),
                    vec![
                        s_return(e_bool(true)),
                    ],
                    vec![],
                    None,
                ),
                s_assign("walk", e_prop(e_var("walk"), "parentNode")),
            ]),
            s_return(e_bool(false)),
        ])
}

/// `getRootNode` lifted out of `decl_class_dom_node` so it builds in its own stack frame.
fn decl_class_dom_node_m19() -> MethodBuilder {
    method("getRootNode")
        .param_default("options", t_array(), e_array(vec![]))
        .keep_unread_params()
        .returns(t_class("\\Dom\\Node"))
        .body(vec![
            s_assign("root", e_this()),
            s_while(e_binop(e_prop(e_var("root"), "parentNode"), BinOp::StrictNotEq, e_null()), vec![
                s_assign("root", e_prop(e_var("root"), "parentNode")),
            ]),
            s_return(e_var("root")),
        ])
}

/// `appendChild` lifted out of `decl_class_dom_node` so it builds in its own stack frame.
fn decl_class_dom_node_m20() -> MethodBuilder {
    method("appendChild")
        .param("node", t_class("\\Dom\\Node"))
        .returns(t_class("\\Dom\\Node"))
        .body(vec![
            s_expr(e_method_call(e_this(), "insertBefore", vec![e_var("node"), e_null()])),
            s_return(e_var("node")),
        ])
}

/// `isBuilding` lifted out of `decl_class_dom_node` so it builds in its own stack frame.
fn decl_class_dom_node_m21() -> MethodBuilder {
    method("isBuilding")
        .returns(TypeExpr::Bool)
        .body(vec![
            s_assign("doc", e_ternary(e_instance_of(e_this(), "\\Dom\\Document"), e_this(), e_this_prop("ownerDocument"))),
            s_return(e_binop(e_binop(e_var("doc"), BinOp::StrictNotEq, e_null()), BinOp::And, e_prop(e_var("doc"), "building"))),
        ])
}

/// `insertBefore` lifted out of `decl_class_dom_node` so it builds in its own stack frame.
fn decl_class_dom_node_m22() -> MethodBuilder {
    method("insertBefore")
        .param("node", t_class("\\Dom\\Node"))
        .param_default("child", t_nullable(t_class("\\Dom\\Node")), e_null())
        .returns(t_class("\\Dom\\Node"))
        .body(vec![
            s_if(
                e_binop(e_prop(e_var("node"), "parentNode"), BinOp::StrictNotEq, e_null()),
                vec![
                    s_expr(e_method_call(e_prop(e_var("node"), "parentNode"), "removeChild", vec![e_var("node")])),
                ],
                vec![],
                None,
            ),
            s_assign("index", e_call("count", vec![e_this_prop("kids")])),
            s_if(
                e_binop(e_var("child"), BinOp::StrictNotEq, e_null()),
                vec![
                    s_assign("found", e_call("array_search", vec![e_var("child"), e_this_prop("kids"), e_bool(true)])),
                    s_if(
                        e_binop(e_var("found"), BinOp::StrictEq, e_bool(false)),
                        vec![
                            s_throw(e_new_fq("Exception", vec![e_str("Not Found Error"), e_int(8)])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("index", e_var("found")),
                ],
                vec![],
                None,
            ),
            s_prop_assign(e_var("node"), "parentNode", e_this()),
            s_prop_assign(e_var("node"), "parentElement", e_ternary(e_instance_of(e_this(), "\\Dom\\Element"), e_this(), e_null())),
            s_if(
                e_binop(e_binop(e_var("child"), BinOp::StrictEq, e_null()), BinOp::And, e_method_call(e_this(), "isBuilding", vec![])),
                vec![
                    s_assign("last", e_this_prop("lastChild")),
                    s_prop_array_push(e_this(), "kids", e_var("node")),
                    s_prop_assign(e_var("node"), "previousSibling", e_var("last")),
                    s_prop_assign(e_var("node"), "nextSibling", e_null()),
                    s_if(
                        e_binop(e_var("last"), BinOp::StrictNotEq, e_null()),
                        vec![
                            s_prop_assign(e_var("last"), "nextSibling", e_var("node")),
                        ],
                        vec![],
                        Some(vec![
                        s_prop_assign(e_this(), "firstChild", e_var("node")),
                    ]),
                    ),
                    s_prop_assign(e_this(), "lastChild", e_var("node")),
                    s_return(e_var("node")),
                ],
                vec![],
                None,
            ),
            s_expr(e_call("array_splice", vec![e_this_prop("kids"), e_var("index"), e_int(0), e_array(vec![e_var("node")])])),
            s_expr(e_method_call(e_this(), "relink", vec![])),
            s_return(e_var("node")),
        ])
}

/// `removeChild` lifted out of `decl_class_dom_node` so it builds in its own stack frame.
fn decl_class_dom_node_m23() -> MethodBuilder {
    method("removeChild")
        .param("child", t_class("\\Dom\\Node"))
        .returns(t_class("\\Dom\\Node"))
        .body(vec![
            s_assign("found", e_call("array_search", vec![e_var("child"), e_this_prop("kids"), e_bool(true)])),
            s_if(
                e_binop(e_var("found"), BinOp::StrictEq, e_bool(false)),
                vec![
                    s_throw(e_new_fq("Exception", vec![e_str("Not Found Error"), e_int(8)])),
                ],
                vec![],
                None,
            ),
            s_expr(e_call("array_splice", vec![e_this_prop("kids"), e_var("found"), e_int(1)])),
            s_prop_assign(e_var("child"), "parentNode", e_null()),
            s_prop_assign(e_var("child"), "parentElement", e_null()),
            s_prop_assign(e_var("child"), "previousSibling", e_null()),
            s_prop_assign(e_var("child"), "nextSibling", e_null()),
            s_expr(e_method_call(e_this(), "relink", vec![])),
            s_return(e_var("child")),
        ])
}

/// `relink` lifted out of `decl_class_dom_node` so it builds in its own stack frame.
fn decl_class_dom_node_m24() -> MethodBuilder {
    method("relink")
        .returns(TypeExpr::Void)
        .body(vec![
            s_assign("count", e_call("count", vec![e_this_prop("kids")])),
            s_for(Some(s_assign("i", e_int(0))), Some(e_binop(e_var("i"), BinOp::Lt, e_var("count"))), Some(s_expr(e_post_inc("i"))), vec![
                s_assign("kid", e_index(e_this_prop("kids"), e_var("i"))),
                s_prop_assign(e_var("kid"), "previousSibling", e_ternary(e_binop(e_var("i"), BinOp::Gt, e_int(0)), e_index(e_this_prop("kids"), e_binop(e_var("i"), BinOp::Sub, e_int(1))), e_null())),
                s_prop_assign(e_var("kid"), "nextSibling", e_ternary(e_binop(e_var("i"), BinOp::Lt, e_binop(e_var("count"), BinOp::Sub, e_int(1))), e_index(e_this_prop("kids"), e_binop(e_var("i"), BinOp::Add, e_int(1))), e_null())),
            ]),
            s_prop_assign(e_this(), "firstChild", e_ternary(e_binop(e_var("count"), BinOp::Gt, e_int(0)), e_index(e_this_prop("kids"), e_int(0)), e_null())),
            s_prop_assign(e_this(), "lastChild", e_ternary(e_binop(e_var("count"), BinOp::Gt, e_int(0)), e_index(e_this_prop("kids"), e_binop(e_var("count"), BinOp::Sub, e_int(1))), e_null())),
            s_if(
                e_method_call(e_this(), "isBuilding", vec![]),
                vec![
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_prop_assign(e_this(), "childNodes", e_new_fq("Dom\\NodeList", vec![e_this_prop("kids")])),
            s_expr(e_method_call(e_this(), "afterRelink", vec![])),
            s_expr(e_method_call(e_this(), "refreshText", vec![])),
            s_if(
                e_binop(e_instance_of(e_this(), "\\Dom\\Element"), BinOp::And, e_instance_of(e_this_prop("parentNode"), "\\Dom\\Document")),
                vec![
                    s_expr(e_method_call(e_this_prop("parentNode"), "afterRelink", vec![])),
                ],
                vec![],
                None,
            ),
        ])
}

/// `finishBuild` lifted out of `decl_class_dom_node` so it builds in its own stack frame.
fn decl_class_dom_node_m25() -> MethodBuilder {
    method("finishBuild")
        .returns(TypeExpr::Void)
        .body(vec![
            s_foreach(e_this_prop("kids"), None, "kid", vec![
                s_expr(e_method_call(e_var("kid"), "finishBuild", vec![])),
            ]),
            s_prop_assign(e_this(), "childNodes", e_new_fq("Dom\\NodeList", vec![e_this_prop("kids")])),
            s_expr(e_method_call(e_this(), "afterRelink", vec![])),
            s_expr(e_method_call(e_this(), "computeText", vec![])),
        ])
}

/// `afterRelink` lifted out of `decl_class_dom_node` so it builds in its own stack frame.
fn decl_class_dom_node_m26() -> MethodBuilder {
    method("afterRelink")
        .returns(TypeExpr::Void)
}

/// `refreshText` lifted out of `decl_class_dom_node` so it builds in its own stack frame.
fn decl_class_dom_node_m27() -> MethodBuilder {
    method("refreshText")
        .returns(TypeExpr::Void)
        .body(vec![
            s_if(
                e_method_call(e_this(), "isBuilding", vec![]),
                vec![
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_assign("walk", e_this()),
            s_while(e_binop(e_var("walk"), BinOp::StrictNotEq, e_null()), vec![
                s_expr(e_method_call(e_var("walk"), "computeText", vec![])),
                s_assign("walk", e_prop(e_var("walk"), "parentNode")),
            ]),
        ])
}

/// `computeText` lifted out of `decl_class_dom_node` so it builds in its own stack frame.
fn decl_class_dom_node_m28() -> MethodBuilder {
    method("computeText")
        .returns(TypeExpr::Void)
        .body(vec![
            s_assign("text", e_str("")),
            s_foreach(e_this_prop("kids"), None, "kid", vec![
                s_if(
                    e_instance_of(e_var("kid"), "\\Dom\\Text"),
                    vec![
                        s_assign("text", e_binop(e_var("text"), BinOp::Concat, e_prop(e_var("kid"), "data"))),
                    ],
                    vec![
                    (e_instance_of(e_var("kid"), "\\Dom\\Element"), vec![
                        s_assign("text", e_binop(e_var("text"), BinOp::Concat, e_prop(e_var("kid"), "textContent"))),
                    ]),
                ],
                    None,
                ),
            ]),
            s_prop_assign(e_this(), "textContent", e_var("text")),
        ])
}

/// `setData` lifted out of `decl_class_dom_characterdata` so it builds in its own stack frame.
fn decl_class_dom_characterdata_m29() -> MethodBuilder {
    method("setData")
        .param("data", TypeExpr::Str)
        .returns(TypeExpr::Void)
        .body(vec![
            s_prop_assign(e_this(), "data", e_var("data")),
            s_prop_assign(e_this(), "nodeValue", e_var("data")),
            s_prop_assign(e_this(), "length", e_call("strlen", vec![e_var("data")])),
            s_prop_assign(e_this(), "textContent", e_var("data")),
        ])
}

/// `computeText` lifted out of `decl_class_dom_characterdata` so it builds in its own stack frame.
fn decl_class_dom_characterdata_m30() -> MethodBuilder {
    method("computeText")
        .returns(TypeExpr::Void)
        .body(vec![
            s_prop_assign(e_this(), "textContent", e_this_prop("data")),
        ])
}

/// `__construct` lifted out of `decl_class_dom_text` so it builds in its own stack frame.
fn decl_class_dom_text_m31() -> MethodBuilder {
    method("__construct")
        .param_default("data", TypeExpr::Str, e_str(""))
        .body(vec![
            s_expr(e_parent_call("__construct", vec![])),
            s_prop_assign(e_this(), "nodeType", e_int(3)),
            s_prop_assign(e_this(), "nodeName", e_str("#text")),
            s_expr(e_method_call(e_this(), "setData", vec![e_var("data")])),
            s_prop_assign(e_this(), "wholeText", e_var("data")),
        ])
}

/// `__construct` lifted out of `decl_class_dom_comment` so it builds in its own stack frame.
fn decl_class_dom_comment_m32() -> MethodBuilder {
    method("__construct")
        .param_default("data", TypeExpr::Str, e_str(""))
        .body(vec![
            s_expr(e_parent_call("__construct", vec![])),
            s_prop_assign(e_this(), "nodeType", e_int(8)),
            s_prop_assign(e_this(), "nodeName", e_str("#comment")),
            s_expr(e_method_call(e_this(), "setData", vec![e_var("data")])),
        ])
}

/// `__construct` lifted out of `decl_class_dom_documenttype` so it builds in its own stack frame.
fn decl_class_dom_documenttype_m33() -> MethodBuilder {
    method("__construct")
        .param_default("name", TypeExpr::Str, e_str(""))
        .param_default("publicId", TypeExpr::Str, e_str(""))
        .param_default("systemId", TypeExpr::Str, e_str(""))
        .body(vec![
            s_expr(e_parent_call("__construct", vec![])),
            s_prop_assign(e_this(), "nodeType", e_int(10)),
            s_prop_assign(e_this(), "nodeName", e_var("name")),
            s_prop_assign(e_this(), "name", e_var("name")),
            s_prop_assign(e_this(), "publicId", e_var("publicId")),
            s_prop_assign(e_this(), "systemId", e_var("systemId")),
        ])
}

/// `__construct` lifted out of `decl_class_dom_attr` so it builds in its own stack frame.
fn decl_class_dom_attr_m34() -> MethodBuilder {
    method("__construct")
        .param_default("name", TypeExpr::Str, e_str(""))
        .param_default("value", TypeExpr::Str, e_str(""))
        .body(vec![
            s_expr(e_parent_call("__construct", vec![])),
            s_prop_assign(e_this(), "nodeType", e_int(2)),
            s_prop_assign(e_this(), "nodeName", e_var("name")),
            s_prop_assign(e_this(), "localName", e_var("name")),
            s_prop_assign(e_this(), "name", e_var("name")),
            s_prop_assign(e_this(), "value", e_var("value")),
            s_prop_assign(e_this(), "nodeValue", e_var("value")),
            s_prop_assign(e_this(), "textContent", e_var("value")),
        ])
}

/// `computeText` lifted out of `decl_class_dom_attr` so it builds in its own stack frame.
fn decl_class_dom_attr_m35() -> MethodBuilder {
    method("computeText")
        .returns(TypeExpr::Void)
        .body(vec![
            s_prop_assign(e_this(), "textContent", e_this_prop("value")),
        ])
}

/// `__construct` lifted out of `decl_class_dom_element` so it builds in its own stack frame.
fn decl_class_dom_element_m36() -> MethodBuilder {
    method("__construct")
        .param_default("qualifiedName", TypeExpr::Str, e_str(""))
        .body(vec![
            s_expr(e_parent_call("__construct", vec![])),
            s_prop_assign(e_this(), "nodeType", e_int(1)),
            s_assign("name", e_call("strtolower", vec![e_var("qualifiedName")])),
            s_prop_assign(e_this(), "localName", e_var("name")),
            s_prop_assign(e_this(), "tagName", e_call("strtoupper", vec![e_var("qualifiedName")])),
            s_prop_assign(e_this(), "nodeName", e_this_prop("tagName")),
            s_prop_assign(e_this(), "attributes", e_new_fq("Dom\\NamedNodeMap", vec![e_array(vec![])])),
            s_prop_assign(e_this(), "children", e_new_fq("Dom\\HTMLCollection", vec![e_array(vec![])])),
        ])
}

/// `afterRelink` lifted out of `decl_class_dom_element` so it builds in its own stack frame.
fn decl_class_dom_element_m37() -> MethodBuilder {
    method("afterRelink")
        .returns(TypeExpr::Void)
        .body(vec![
            s_assign("elements", e_array(vec![])),
            s_foreach(e_this_prop("kids"), None, "kid", vec![
                s_if(
                    e_instance_of(e_var("kid"), "\\Dom\\Element"),
                    vec![
                        s_array_push("elements", e_var("kid")),
                    ],
                    vec![],
                    None,
                ),
            ]),
            s_assign("count", e_call("count", vec![e_var("elements")])),
            s_for(Some(s_assign("i", e_int(0))), Some(e_binop(e_var("i"), BinOp::Lt, e_var("count"))), Some(s_expr(e_post_inc("i"))), vec![
                s_prop_assign(e_index(e_var("elements"), e_var("i")), "previousElementSibling", e_ternary(e_binop(e_var("i"), BinOp::Gt, e_int(0)), e_index(e_var("elements"), e_binop(e_var("i"), BinOp::Sub, e_int(1))), e_null())),
                s_prop_assign(e_index(e_var("elements"), e_var("i")), "nextElementSibling", e_ternary(e_binop(e_var("i"), BinOp::Lt, e_binop(e_var("count"), BinOp::Sub, e_int(1))), e_index(e_var("elements"), e_binop(e_var("i"), BinOp::Add, e_int(1))), e_null())),
            ]),
            s_prop_assign(e_this(), "children", e_new_fq("Dom\\HTMLCollection", vec![e_var("elements")])),
            s_prop_assign(e_this(), "firstElementChild", e_ternary(e_binop(e_var("count"), BinOp::Gt, e_int(0)), e_index(e_var("elements"), e_int(0)), e_null())),
            s_prop_assign(e_this(), "lastElementChild", e_ternary(e_binop(e_var("count"), BinOp::Gt, e_int(0)), e_index(e_var("elements"), e_binop(e_var("count"), BinOp::Sub, e_int(1))), e_null())),
            s_prop_assign(e_this(), "childElementCount", e_var("count")),
        ])
}

/// `hasAttributes` lifted out of `decl_class_dom_element` so it builds in its own stack frame.
fn decl_class_dom_element_m38() -> MethodBuilder {
    method("hasAttributes")
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_binop(e_prop(e_this_prop("attributes"), "length"), BinOp::Gt, e_int(0))),
        ])
}

/// `getAttributeNames` lifted out of `decl_class_dom_element` so it builds in its own stack frame.
fn decl_class_dom_element_m39() -> MethodBuilder {
    method("getAttributeNames")
        .returns(t_array())
        .body(vec![
            s_assign("names", e_array(vec![])),
            s_foreach(e_prop(e_this_prop("attributes"), "items"), None, "attr", vec![
                s_array_push("names", e_prop(e_var("attr"), "name")),
            ]),
            s_return(e_var("names")),
        ])
}

/// `getAttribute` lifted out of `decl_class_dom_element` so it builds in its own stack frame.
fn decl_class_dom_element_m40() -> MethodBuilder {
    method("getAttribute")
        .param("qualifiedName", TypeExpr::Str)
        .returns(t_nullable(TypeExpr::Str))
        .body(vec![
            s_assign("attr", e_method_call(e_this_prop("attributes"), "getNamedItem", vec![e_var("qualifiedName")])),
            s_return(e_ternary(e_binop(e_var("attr"), BinOp::StrictEq, e_null()), e_null(), e_prop(e_var("attr"), "value"))),
        ])
}

/// `hasAttribute` lifted out of `decl_class_dom_element` so it builds in its own stack frame.
fn decl_class_dom_element_m41() -> MethodBuilder {
    method("hasAttribute")
        .param("qualifiedName", TypeExpr::Str)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_binop(e_method_call(e_this_prop("attributes"), "getNamedItem", vec![e_var("qualifiedName")]), BinOp::StrictNotEq, e_null())),
        ])
}

/// `getAttributeNode` lifted out of `decl_class_dom_element` so it builds in its own stack frame.
fn decl_class_dom_element_m42() -> MethodBuilder {
    method("getAttributeNode")
        .param("qualifiedName", TypeExpr::Str)
        .returns(t_nullable(t_class("\\Dom\\Attr")))
        .body(vec![
            s_return(e_method_call(e_this_prop("attributes"), "getNamedItem", vec![e_var("qualifiedName")])),
        ])
}

/// `setAttribute` lifted out of `decl_class_dom_element` so it builds in its own stack frame.
fn decl_class_dom_element_m43() -> MethodBuilder {
    method("setAttribute")
        .param("qualifiedName", TypeExpr::Str)
        .param("value", TypeExpr::Str)
        .returns(TypeExpr::Void)
        .body(vec![
            s_assign("name", e_ternary(e_binop(e_this_prop("namespaceURI"), BinOp::StrictEq, e_str("http://www.w3.org/1999/xhtml")), e_call("strtolower", vec![e_var("qualifiedName")]), e_var("qualifiedName"))),
            s_assign("items", e_prop(e_this_prop("attributes"), "items")),
            s_foreach(e_var("items"), None, "attr", vec![
                s_if(
                    e_binop(e_prop(e_var("attr"), "name"), BinOp::StrictEq, e_var("name")),
                    vec![
                        s_prop_assign(e_var("attr"), "value", e_var("value")),
                        s_prop_assign(e_var("attr"), "nodeValue", e_var("value")),
                        s_prop_assign(e_var("attr"), "textContent", e_var("value")),
                        s_expr(e_method_call(e_this(), "noteAttribute", vec![e_var("name"), e_var("value")])),
                        s_return_void(),
                    ],
                    vec![],
                    None,
                ),
            ]),
            s_assign("attr", e_new_fq("Dom\\Attr", vec![e_var("name"), e_var("value")])),
            s_prop_assign(e_var("attr"), "ownerElement", e_this()),
            s_prop_assign(e_var("attr"), "ownerDocument", e_this_prop("ownerDocument")),
            s_array_push("items", e_var("attr")),
            s_prop_assign(e_this(), "attributes", e_new_fq("Dom\\NamedNodeMap", vec![e_var("items")])),
            s_expr(e_method_call(e_this(), "noteAttribute", vec![e_var("name"), e_var("value")])),
        ])
}

/// `removeAttribute` lifted out of `decl_class_dom_element` so it builds in its own stack frame.
fn decl_class_dom_element_m44() -> MethodBuilder {
    method("removeAttribute")
        .param("qualifiedName", TypeExpr::Str)
        .returns(TypeExpr::Void)
        .body(vec![
            s_assign("name", e_ternary(e_binop(e_this_prop("namespaceURI"), BinOp::StrictEq, e_str("http://www.w3.org/1999/xhtml")), e_call("strtolower", vec![e_var("qualifiedName")]), e_var("qualifiedName"))),
            s_assign("items", e_array(vec![])),
            s_foreach(e_prop(e_this_prop("attributes"), "items"), None, "attr", vec![
                s_if(
                    e_binop(e_prop(e_var("attr"), "name"), BinOp::StrictNotEq, e_var("name")),
                    vec![
                        s_array_push("items", e_var("attr")),
                    ],
                    vec![],
                    None,
                ),
            ]),
            s_prop_assign(e_this(), "attributes", e_new_fq("Dom\\NamedNodeMap", vec![e_var("items")])),
            s_if(
                e_binop(e_var("name"), BinOp::StrictEq, e_str("id")),
                vec![
                    s_prop_assign(e_this(), "id", e_str("")),
                ],
                vec![
                (e_binop(e_var("name"), BinOp::StrictEq, e_str("class")), vec![
                    s_prop_assign(e_this(), "className", e_str("")),
                ]),
            ],
                None,
            ),
        ])
}

/// `noteAttribute` lifted out of `decl_class_dom_element` so it builds in its own stack frame.
fn decl_class_dom_element_m45() -> MethodBuilder {
    method("noteAttribute")
        .private()
        .param("name", TypeExpr::Str)
        .param("value", TypeExpr::Str)
        .returns(TypeExpr::Void)
        .body(vec![
            s_if(
                e_binop(e_var("name"), BinOp::StrictEq, e_str("id")),
                vec![
                    s_prop_assign(e_this(), "id", e_var("value")),
                ],
                vec![
                (e_binop(e_var("name"), BinOp::StrictEq, e_str("class")), vec![
                    s_prop_assign(e_this(), "className", e_var("value")),
                ]),
            ],
                None,
            ),
        ])
}

/// `getElementsByTagName` lifted out of `decl_class_dom_element` so it builds in its own stack frame.
fn decl_class_dom_element_m46() -> MethodBuilder {
    method("getElementsByTagName")
        .param("qualifiedName", TypeExpr::Str)
        .returns(t_class("\\Dom\\HTMLCollection"))
        .body(vec![
            s_assign("found", e_array(vec![])),
            s_expr(e_method_call(e_this(), "collectByTagName", vec![e_call("strtolower", vec![e_var("qualifiedName")]), e_var("found")])),
            s_return(e_new_fq("Dom\\HTMLCollection", vec![e_var("found")])),
        ])
}

/// `collectByTagName` lifted out of `decl_class_dom_element` so it builds in its own stack frame.
fn decl_class_dom_element_m47() -> MethodBuilder {
    method("collectByTagName")
        .param("wanted", TypeExpr::Str)
        .param_by_ref("found", Some(t_array()))
        .returns(TypeExpr::Void)
        .body(vec![
            s_foreach(e_this_prop("kids"), None, "kid", vec![
                s_if(
                    e_instance_of(e_var("kid"), "\\Dom\\Element"),
                    vec![
                        s_if(
                            e_binop(e_binop(e_var("wanted"), BinOp::StrictEq, e_str("*")), BinOp::Or, e_binop(e_prop(e_var("kid"), "localName"), BinOp::StrictEq, e_var("wanted"))),
                            vec![
                                s_array_push("found", e_var("kid")),
                            ],
                            vec![],
                            None,
                        ),
                        s_expr(e_method_call(e_var("kid"), "collectByTagName", vec![e_var("wanted"), e_var("found")])),
                    ],
                    vec![],
                    None,
                ),
            ]),
        ])
}

/// `__construct` lifted out of `decl_class_dom_documentfragment` so it builds in its own stack frame.
fn decl_class_dom_documentfragment_m48() -> MethodBuilder {
    method("__construct")
        .body(vec![
            s_expr(e_parent_call("__construct", vec![])),
            s_prop_assign(e_this(), "nodeType", e_int(11)),
            s_prop_assign(e_this(), "nodeName", e_str("#document-fragment")),
        ])
}

/// `__construct` lifted out of `decl_class_dom_document` so it builds in its own stack frame.
fn decl_class_dom_document_m49() -> MethodBuilder {
    method("__construct")
        .body(vec![
            s_expr(e_parent_call("__construct", vec![])),
            s_prop_assign(e_this(), "nodeType", e_int(9)),
            s_prop_assign(e_this(), "nodeName", e_str("#document")),
        ])
}

/// `afterRelink` lifted out of `decl_class_dom_document` so it builds in its own stack frame.
fn decl_class_dom_document_m50() -> MethodBuilder {
    method("afterRelink")
        .returns(TypeExpr::Void)
        .body(vec![
            s_prop_assign(e_this(), "doctype", e_null()),
            s_prop_assign(e_this(), "documentElement", e_null()),
            s_foreach(e_this_prop("kids"), None, "kid", vec![
                s_if(
                    e_binop(e_instance_of(e_var("kid"), "\\Dom\\DocumentType"), BinOp::And, e_binop(e_this_prop("doctype"), BinOp::StrictEq, e_null())),
                    vec![
                        s_prop_assign(e_this(), "doctype", e_var("kid")),
                    ],
                    vec![
                    (e_binop(e_instance_of(e_var("kid"), "\\Dom\\Element"), BinOp::And, e_binop(e_this_prop("documentElement"), BinOp::StrictEq, e_null())), vec![
                        s_prop_assign(e_this(), "documentElement", e_var("kid")),
                    ]),
                ],
                    None,
                ),
            ]),
            s_prop_assign(e_this(), "head", e_null()),
            s_prop_assign(e_this(), "body", e_null()),
            s_if(
                e_binop(e_binop(e_this_prop("documentElement"), BinOp::StrictNotEq, e_null()), BinOp::And, e_binop(e_prop(e_this_prop("documentElement"), "localName"), BinOp::StrictEq, e_str("html"))),
                vec![
                    s_foreach(e_prop(e_this_prop("documentElement"), "kids"), None, "kid", vec![
                        s_if(
                            e_instance_of(e_var("kid"), "\\Dom\\Element"),
                            vec![
                                s_if(
                                    e_binop(e_binop(e_prop(e_var("kid"), "localName"), BinOp::StrictEq, e_str("head")), BinOp::And, e_binop(e_this_prop("head"), BinOp::StrictEq, e_null())),
                                    vec![
                                        s_prop_assign(e_this(), "head", e_var("kid")),
                                    ],
                                    vec![
                                    (e_binop(e_binop(e_binop(e_prop(e_var("kid"), "localName"), BinOp::StrictEq, e_str("body")), BinOp::Or, e_binop(e_prop(e_var("kid"), "localName"), BinOp::StrictEq, e_str("frameset"))), BinOp::And, e_binop(e_this_prop("body"), BinOp::StrictEq, e_null())), vec![
                                        s_prop_assign(e_this(), "body", e_var("kid")),
                                    ]),
                                ],
                                    None,
                                ),
                            ],
                            vec![],
                            None,
                        ),
                    ]),
                ],
                vec![],
                None,
            ),
        ])
}

/// `computeText` lifted out of `decl_class_dom_document` so it builds in its own stack frame.
fn decl_class_dom_document_m51() -> MethodBuilder {
    method("computeText")
        .returns(TypeExpr::Void)
        .body(vec![
            s_prop_assign(e_this(), "textContent", e_null()),
        ])
}

/// `getElementsByTagName` lifted out of `decl_class_dom_document` so it builds in its own stack frame.
fn decl_class_dom_document_m52() -> MethodBuilder {
    method("getElementsByTagName")
        .param("qualifiedName", TypeExpr::Str)
        .returns(t_class("\\Dom\\HTMLCollection"))
        .body(vec![
            s_assign("found", e_array(vec![])),
            s_assign("wanted", e_call("strtolower", vec![e_var("qualifiedName")])),
            s_if(
                e_binop(e_this_prop("documentElement"), BinOp::StrictNotEq, e_null()),
                vec![
                    s_if(
                        e_binop(e_binop(e_var("wanted"), BinOp::StrictEq, e_str("*")), BinOp::Or, e_binop(e_prop(e_this_prop("documentElement"), "localName"), BinOp::StrictEq, e_var("wanted"))),
                        vec![
                            s_array_push("found", e_this_prop("documentElement")),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this_prop("documentElement"), "collectByTagName", vec![e_var("wanted"), e_var("found")])),
                ],
                vec![],
                None,
            ),
            s_return(e_new_fq("Dom\\HTMLCollection", vec![e_var("found")])),
        ])
}

/// `createElement` lifted out of `decl_class_dom_document` so it builds in its own stack frame.
fn decl_class_dom_document_m53() -> MethodBuilder {
    method("createElement")
        .param("localName", TypeExpr::Str)
        .returns(t_class("\\Dom\\Element"))
        .body(vec![
            s_assign("element", e_new_fq("Dom\\Element", vec![e_var("localName")])),
            s_prop_assign(e_var("element"), "ownerDocument", e_this()),
            s_return(e_var("element")),
        ])
}

/// `createTextNode` lifted out of `decl_class_dom_document` so it builds in its own stack frame.
fn decl_class_dom_document_m54() -> MethodBuilder {
    method("createTextNode")
        .param("data", TypeExpr::Str)
        .returns(t_class("\\Dom\\Text"))
        .body(vec![
            s_assign("text", e_new_fq("Dom\\Text", vec![e_var("data")])),
            s_prop_assign(e_var("text"), "ownerDocument", e_this()),
            s_return(e_var("text")),
        ])
}

/// `createComment` lifted out of `decl_class_dom_document` so it builds in its own stack frame.
fn decl_class_dom_document_m55() -> MethodBuilder {
    method("createComment")
        .param("data", TypeExpr::Str)
        .returns(t_class("\\Dom\\Comment"))
        .body(vec![
            s_assign("comment", e_new_fq("Dom\\Comment", vec![e_var("data")])),
            s_prop_assign(e_var("comment"), "ownerDocument", e_this()),
            s_return(e_var("comment")),
        ])
}

/// `getElementById` lifted out of `decl_class_dom_document` so it builds in its own stack frame.
fn decl_class_dom_document_m56() -> MethodBuilder {
    method("getElementById")
        .param("elementId", TypeExpr::Str)
        .returns(t_nullable(t_class("\\Dom\\Element")))
        .body(vec![
            s_if(
                e_binop(e_binop(e_var("elementId"), BinOp::StrictEq, e_str("")), BinOp::Or, e_binop(e_this_prop("documentElement"), BinOp::StrictEq, e_null())),
                vec![
                    s_return(e_null()),
                ],
                vec![],
                None,
            ),
            s_assign("found", e_array(vec![])),
            s_expr(e_method_call(e_this_prop("documentElement"), "collectByTagName", vec![e_str("*"), e_var("found")])),
            s_if(
                e_binop(e_prop(e_this_prop("documentElement"), "id"), BinOp::StrictEq, e_var("elementId")),
                vec![
                    s_return(e_this_prop("documentElement")),
                ],
                vec![],
                None,
            ),
            s_foreach(e_var("found"), None, "element", vec![
                s_if(
                    e_binop(e_prop(e_var("element"), "id"), BinOp::StrictEq, e_var("elementId")),
                    vec![
                        s_return(e_var("element")),
                    ],
                    vec![],
                    None,
                ),
            ]),
            s_return(e_null()),
        ])
}

/// `createEmpty` lifted out of `decl_class_dom_htmldocument` so it builds in its own stack frame.
fn decl_class_dom_htmldocument_m57() -> MethodBuilder {
    method("createEmpty")
        .static_()
        .param_default("encoding", TypeExpr::Str, e_str("UTF-8"))
        .keep_unread_params()
        .returns(t_class("\\Dom\\HTMLDocument"))
        .body(vec![
            s_assign("document", e_new_fq("Dom\\HTMLDocument", vec![])),
            s_return(e_var("document")),
        ])
}

/// `createFromString` lifted out of `decl_class_dom_htmldocument` so it builds in its own stack frame.
fn decl_class_dom_htmldocument_m58() -> MethodBuilder {
    method("createFromString")
        .static_()
        .param("source", TypeExpr::Str)
        .param_default("options", TypeExpr::Int, e_int(0))
        .param_default("overrideEncoding", t_nullable(TypeExpr::Str), e_null())
        .keep_unread_params()
        .returns(t_class("\\Dom\\HTMLDocument"))
        .body(vec![
            s_assign("document", e_new_fq("Dom\\HTMLDocument", vec![])),
            s_prop_assign(e_var("document"), "nodeType", e_int(13)),
            s_prop_assign(e_var("document"), "building", e_bool(true)),
            s_assign("parser", e_new_fq("Dom\\__ElephcHtmlParser", vec![e_var("document"), e_var("source")])),
            s_expr(e_method_call(e_var("parser"), "run", vec![])),
            s_prop_assign(e_var("document"), "building", e_bool(false)),
            s_expr(e_method_call(e_var("document"), "finishBuild", vec![])),
            s_return(e_var("document")),
        ])
}

/// `__construct` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m59() -> MethodBuilder {
    method("__construct")
        .param("doc", t_class("\\Dom\\Document"))
        .param("source", TypeExpr::Str)
        .body(vec![
            s_prop_assign(e_this(), "doc", e_var("doc")),
            s_assign("source", e_call("str_replace", vec![e_str("\r\n"), e_str("\n"), e_var("source")])),
            s_assign("source", e_call("str_replace", vec![e_str("\r"), e_str("\n"), e_var("source")])),
            s_prop_assign(e_this(), "src", e_var("source")),
            s_prop_assign(e_this(), "len", e_call("strlen", vec![e_var("source")])),
        ])
}

/// `run` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m60() -> MethodBuilder {
    method("run")
        .returns(TypeExpr::Void)
        .body(vec![
            s_while(e_binop(e_not(e_this_prop("stop")), BinOp::And, e_binop(e_this_prop("pos"), BinOp::Lt, e_this_prop("len"))), vec![
                s_if(
                    e_binop(e_this_prop("rawTag"), BinOp::StrictNotEq, e_str("")),
                    vec![
                        s_expr(e_method_call(e_this(), "scanRaw", vec![])),
                        s_continue(1),
                    ],
                    vec![],
                    None,
                ),
                s_assign("lt", e_call("strpos", vec![e_this_prop("src"), e_str("<"), e_this_prop("pos")])),
                s_assign("amp", e_call("strpos", vec![e_this_prop("src"), e_str("&"), e_this_prop("pos")])),
                s_assign("next", e_this_prop("len")),
                s_if(
                    e_binop(e_var("lt"), BinOp::StrictNotEq, e_bool(false)),
                    vec![
                        s_assign("next", e_var("lt")),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_binop(e_var("amp"), BinOp::StrictNotEq, e_bool(false)), BinOp::And, e_binop(e_var("amp"), BinOp::Lt, e_var("next"))),
                    vec![
                        s_assign("next", e_var("amp")),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_var("next"), BinOp::Gt, e_this_prop("pos")),
                    vec![
                        s_expr(e_method_call(e_this(), "emitChars", vec![e_call("substr", vec![e_this_prop("src"), e_this_prop("pos"), e_binop(e_var("next"), BinOp::Sub, e_this_prop("pos"))])])),
                        s_prop_assign(e_this(), "pos", e_var("next")),
                        s_continue(1),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_index(e_this_prop("src"), e_this_prop("pos")), BinOp::StrictEq, e_str("&")),
                    vec![
                        s_expr(e_method_call(e_this(), "emitChars", vec![e_method_call(e_this(), "charRef", vec![e_bool(false)])])),
                        s_continue(1),
                    ],
                    vec![],
                    None,
                ),
                s_expr(e_method_call(e_this(), "tagOpen", vec![])),
            ]),
            s_expr(e_method_call(e_this(), "flushText", vec![])),
            s_if(
                e_not(e_this_prop("stop")),
                vec![
                    s_expr(e_method_call(e_this(), "tbEof", vec![])),
                ],
                vec![],
                None,
            ),
        ])
}

/// `peek` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m61() -> MethodBuilder {
    method("peek")
        .private()
        .param_default("offset", TypeExpr::Int, e_int(0))
        .returns(TypeExpr::Str)
        .body(vec![
            s_assign("at", e_binop(e_this_prop("pos"), BinOp::Add, e_var("offset"))),
            s_if(
                e_binop(e_var("at"), BinOp::GtEq, e_this_prop("len")),
                vec![
                    s_return(e_str("")),
                ],
                vec![],
                None,
            ),
            s_return(e_index(e_this_prop("src"), e_var("at"))),
        ])
}

/// `isSpace` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m62() -> MethodBuilder {
    method("isSpace")
        .private()
        .static_()
        .param("c", TypeExpr::Str)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_binop(e_binop(e_binop(e_binop(e_var("c"), BinOp::StrictEq, e_str(" ")), BinOp::Or, e_binop(e_var("c"), BinOp::StrictEq, e_str("\t"))), BinOp::Or, e_binop(e_var("c"), BinOp::StrictEq, e_str("\n"))), BinOp::Or, e_binop(e_var("c"), BinOp::StrictEq, e_str("\u{c}")))),
        ])
}

/// `isAlpha` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m63() -> MethodBuilder {
    method("isAlpha")
        .private()
        .static_()
        .param("c", TypeExpr::Str)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_binop(e_binop(e_binop(e_var("c"), BinOp::GtEq, e_str("a")), BinOp::And, e_binop(e_var("c"), BinOp::LtEq, e_str("z"))), BinOp::Or, e_binop(e_binop(e_var("c"), BinOp::GtEq, e_str("A")), BinOp::And, e_binop(e_var("c"), BinOp::LtEq, e_str("Z"))))),
        ])
}

/// `tagOpen` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m64() -> MethodBuilder {
    method("tagOpen")
        .private()
        .returns(TypeExpr::Void)
        .body(vec![
            s_assign("c", e_method_call(e_this(), "peek", vec![e_int(1)])),
            s_if(
                e_binop(e_var("c"), BinOp::StrictEq, e_str("")),
                vec![
                    s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                    s_expr(e_method_call(e_this(), "emitChars", vec![e_str("<")])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("c"), BinOp::StrictEq, e_str("!")),
                vec![
                    s_expr(e_method_call(e_this(), "markupDeclaration", vec![])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("c"), BinOp::StrictEq, e_str("/")),
                vec![
                    s_assign("d", e_method_call(e_this(), "peek", vec![e_int(2)])),
                    s_if(
                        e_binop(e_var("d"), BinOp::StrictEq, e_str("")),
                        vec![
                            s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(2))),
                            s_expr(e_method_call(e_this(), "emitChars", vec![e_str("</")])),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_self_call("isAlpha", vec![e_var("d")]),
                        vec![
                            s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(2))),
                            s_expr(e_method_call(e_this(), "readTag", vec![e_bool(false)])),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("d"), BinOp::StrictEq, e_str(">")),
                        vec![
                            s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(3))),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(2))),
                    s_expr(e_method_call(e_this(), "bogusComment", vec![e_str("")])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_self_call("isAlpha", vec![e_var("c")]),
                vec![
                    s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                    s_expr(e_method_call(e_this(), "readTag", vec![e_bool(true)])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("c"), BinOp::StrictEq, e_str("?")),
                vec![
                    s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                    s_expr(e_method_call(e_this(), "bogusComment", vec![e_str("")])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
            s_expr(e_method_call(e_this(), "emitChars", vec![e_str("<")])),
        ])
}

/// `bogusComment` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m65() -> MethodBuilder {
    method("bogusComment")
        .private()
        .param("prefix", TypeExpr::Str)
        .returns(TypeExpr::Void)
        .body(vec![
            s_assign("end", e_call("strpos", vec![e_this_prop("src"), e_str(">"), e_this_prop("pos")])),
            s_if(
                e_binop(e_var("end"), BinOp::StrictEq, e_bool(false)),
                vec![
                    s_assign("text", e_binop(e_var("prefix"), BinOp::Concat, e_call("substr", vec![e_this_prop("src"), e_this_prop("pos")]))),
                    s_prop_assign(e_this(), "pos", e_this_prop("len")),
                ],
                vec![],
                Some(vec![
                s_assign("text", e_binop(e_var("prefix"), BinOp::Concat, e_call("substr", vec![e_this_prop("src"), e_this_prop("pos"), e_binop(e_var("end"), BinOp::Sub, e_this_prop("pos"))]))),
                s_prop_assign(e_this(), "pos", e_binop(e_var("end"), BinOp::Add, e_int(1))),
            ]),
            ),
            s_expr(e_method_call(e_this(), "emitComment", vec![e_call("str_replace", vec![e_str("\0"), e_str("�"), e_var("text")])])),
        ])
}

/// `markupDeclaration` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m66() -> MethodBuilder {
    method("markupDeclaration")
        .private()
        .returns(TypeExpr::Void)
        .body(vec![
            s_if(
                e_binop(e_call("substr", vec![e_this_prop("src"), e_binop(e_this_prop("pos"), BinOp::Add, e_int(2)), e_int(2)]), BinOp::StrictEq, e_str("--")),
                vec![
                    s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(4))),
                    s_expr(e_method_call(e_this(), "readComment", vec![])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_call("strtolower", vec![e_call("substr", vec![e_this_prop("src"), e_binop(e_this_prop("pos"), BinOp::Add, e_int(2)), e_int(7)])]), BinOp::StrictEq, e_str("doctype")),
                vec![
                    s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(9))),
                    s_expr(e_method_call(e_this(), "readDoctype", vec![])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_call("substr", vec![e_this_prop("src"), e_binop(e_this_prop("pos"), BinOp::Add, e_int(2)), e_int(7)]), BinOp::StrictEq, e_str("[CDATA[")),
                vec![
                    s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(2))),
                    s_expr(e_method_call(e_this(), "bogusComment", vec![e_str("")])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(2))),
            s_expr(e_method_call(e_this(), "bogusComment", vec![e_str("")])),
        ])
}

/// `readComment` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m67() -> MethodBuilder {
    method("readComment")
        .private()
        .returns(TypeExpr::Void)
        .body(vec![
            s_if(
                e_binop(e_method_call(e_this(), "peek", vec![]), BinOp::StrictEq, e_str(">")),
                vec![
                    s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                    s_expr(e_method_call(e_this(), "emitComment", vec![e_str("")])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_method_call(e_this(), "peek", vec![]), BinOp::StrictEq, e_str("-")), BinOp::And, e_binop(e_method_call(e_this(), "peek", vec![e_int(1)]), BinOp::StrictEq, e_str(">"))),
                vec![
                    s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(2))),
                    s_expr(e_method_call(e_this(), "emitComment", vec![e_str("")])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_assign("start", e_this_prop("pos")),
            s_assign("i", e_this_prop("pos")),
            s_while(e_binop(e_var("i"), BinOp::Lt, e_this_prop("len")), vec![
                s_assign("end", e_call("strpos", vec![e_this_prop("src"), e_str("--"), e_var("i")])),
                s_if(
                    e_binop(e_var("end"), BinOp::StrictEq, e_bool(false)),
                    vec![
                        s_break(1),
                    ],
                    vec![],
                    None,
                ),
                s_assign("after", e_binop(e_var("end"), BinOp::Add, e_int(2))),
                s_assign("c", e_ternary(e_binop(e_var("after"), BinOp::Lt, e_this_prop("len")), e_index(e_this_prop("src"), e_var("after")), e_str(""))),
                s_if(
                    e_binop(e_var("c"), BinOp::StrictEq, e_str(">")),
                    vec![
                        s_prop_assign(e_this(), "pos", e_binop(e_var("after"), BinOp::Add, e_int(1))),
                        s_expr(e_method_call(e_this(), "emitComment", vec![e_call("str_replace", vec![e_str("\0"), e_str("�"), e_call("substr", vec![e_this_prop("src"), e_var("start"), e_binop(e_var("end"), BinOp::Sub, e_var("start"))])])])),
                        s_return_void(),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_binop(e_binop(e_var("c"), BinOp::StrictEq, e_str("!")), BinOp::And, e_binop(e_binop(e_var("after"), BinOp::Add, e_int(1)), BinOp::Lt, e_this_prop("len"))), BinOp::And, e_binop(e_index(e_this_prop("src"), e_binop(e_var("after"), BinOp::Add, e_int(1))), BinOp::StrictEq, e_str(">"))),
                    vec![
                        s_prop_assign(e_this(), "pos", e_binop(e_var("after"), BinOp::Add, e_int(2))),
                        s_expr(e_method_call(e_this(), "emitComment", vec![e_call("str_replace", vec![e_str("\0"), e_str("�"), e_call("substr", vec![e_this_prop("src"), e_var("start"), e_binop(e_var("end"), BinOp::Sub, e_var("start"))])])])),
                        s_return_void(),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_var("c"), BinOp::StrictEq, e_str("-")),
                    vec![
                        s_assign("i", e_binop(e_var("end"), BinOp::Add, e_int(1))),
                        s_continue(1),
                    ],
                    vec![],
                    None,
                ),
                s_assign("i", e_binop(e_var("end"), BinOp::Add, e_int(1))),
            ]),
            s_assign("text", e_call("substr", vec![e_this_prop("src"), e_var("start")])),
            s_prop_assign(e_this(), "pos", e_this_prop("len")),
            s_if(
                e_binop(e_call("substr", vec![e_var("text"), e_neg(e_int(2))]), BinOp::StrictEq, e_str("--")),
                vec![
                    s_assign("text", e_call("substr", vec![e_var("text"), e_int(0), e_neg(e_int(2))])),
                ],
                vec![
                (e_binop(e_call("substr", vec![e_var("text"), e_neg(e_int(1))]), BinOp::StrictEq, e_str("-")), vec![
                    s_assign("text", e_call("substr", vec![e_var("text"), e_int(0), e_neg(e_int(1))])),
                ]),
            ],
                None,
            ),
            s_expr(e_method_call(e_this(), "emitComment", vec![e_call("str_replace", vec![e_str("\0"), e_str("�"), e_var("text")])])),
        ])
}

/// `readDoctype` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m68() -> MethodBuilder {
    method("readDoctype")
        .private()
        .returns(TypeExpr::Void)
        .body(vec![
            s_assign("name", e_str("")),
            s_assign("public", e_null()),
            s_assign("system", e_null()),
            s_assign("quirks", e_bool(false)),
            s_expr(e_method_call(e_this(), "skipSpaces", vec![])),
            s_if(
                e_binop(e_this_prop("pos"), BinOp::GtEq, e_this_prop("len")),
                vec![
                    s_expr(e_method_call(e_this(), "emitDoctype", vec![e_str(""), e_null(), e_null(), e_bool(true)])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_method_call(e_this(), "peek", vec![]), BinOp::StrictEq, e_str(">")),
                vec![
                    s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                    s_expr(e_method_call(e_this(), "emitDoctype", vec![e_str(""), e_null(), e_null(), e_bool(true)])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_while(e_binop(e_this_prop("pos"), BinOp::Lt, e_this_prop("len")), vec![
                s_assign("c", e_index(e_this_prop("src"), e_this_prop("pos"))),
                s_if(
                    e_binop(e_self_call("isSpace", vec![e_var("c")]), BinOp::Or, e_binop(e_var("c"), BinOp::StrictEq, e_str(">"))),
                    vec![
                        s_break(1),
                    ],
                    vec![],
                    None,
                ),
                s_assign("name", e_binop(e_var("name"), BinOp::Concat, e_var("c"))),
                s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
            ]),
            s_assign("name", e_call("strtolower", vec![e_call("str_replace", vec![e_str("\0"), e_str("�"), e_var("name")])])),
            s_expr(e_method_call(e_this(), "skipSpaces", vec![])),
            s_if(
                e_binop(e_this_prop("pos"), BinOp::GtEq, e_this_prop("len")),
                vec![
                    s_expr(e_method_call(e_this(), "emitDoctype", vec![e_var("name"), e_null(), e_null(), e_bool(true)])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_method_call(e_this(), "peek", vec![]), BinOp::StrictNotEq, e_str(">")),
                vec![
                    s_assign("word", e_call("strtolower", vec![e_call("substr", vec![e_this_prop("src"), e_this_prop("pos"), e_int(6)])])),
                    s_if(
                        e_binop(e_var("word"), BinOp::StrictEq, e_str("public")),
                        vec![
                            s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(6))),
                            s_assign("public", e_method_call(e_this(), "doctypeId", vec![])),
                            s_if(
                                e_binop(e_var("public"), BinOp::StrictNotEq, e_null()),
                                vec![
                                    s_assign("system", e_method_call(e_this(), "doctypeId", vec![])),
                                ],
                                vec![],
                                None,
                            ),
                        ],
                        vec![
                        (e_binop(e_var("word"), BinOp::StrictEq, e_str("system")), vec![
                            s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(6))),
                            s_assign("system", e_method_call(e_this(), "doctypeId", vec![])),
                        ]),
                    ],
                        Some(vec![
                        s_assign("quirks", e_bool(true)),
                    ]),
                    ),
                ],
                vec![],
                None,
            ),
            s_assign("end", e_call("strpos", vec![e_this_prop("src"), e_str(">"), e_this_prop("pos")])),
            s_if(
                e_binop(e_var("end"), BinOp::StrictEq, e_bool(false)),
                vec![
                    s_prop_assign(e_this(), "pos", e_this_prop("len")),
                    s_assign("quirks", e_bool(true)),
                ],
                vec![],
                Some(vec![
                s_prop_assign(e_this(), "pos", e_binop(e_var("end"), BinOp::Add, e_int(1))),
            ]),
            ),
            s_expr(e_method_call(e_this(), "emitDoctype", vec![e_var("name"), e_var("public"), e_var("system"), e_var("quirks")])),
        ])
}

/// `doctypeId` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m69() -> MethodBuilder {
    method("doctypeId")
        .private()
        .returns(t_nullable(TypeExpr::Str))
        .body(vec![
            s_expr(e_method_call(e_this(), "skipSpaces", vec![])),
            s_assign("quote", e_method_call(e_this(), "peek", vec![])),
            s_if(
                e_binop(e_binop(e_var("quote"), BinOp::StrictNotEq, e_str("\"")), BinOp::And, e_binop(e_var("quote"), BinOp::StrictNotEq, e_str("'"))),
                vec![
                    s_return(e_null()),
                ],
                vec![],
                None,
            ),
            s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
            s_assign("end", e_call("strpos", vec![e_this_prop("src"), e_var("quote"), e_this_prop("pos")])),
            s_if(
                e_binop(e_var("end"), BinOp::StrictEq, e_bool(false)),
                vec![
                    s_assign("value", e_call("substr", vec![e_this_prop("src"), e_this_prop("pos")])),
                    s_prop_assign(e_this(), "pos", e_this_prop("len")),
                    s_return(e_var("value")),
                ],
                vec![],
                None,
            ),
            s_assign("value", e_call("substr", vec![e_this_prop("src"), e_this_prop("pos"), e_binop(e_var("end"), BinOp::Sub, e_this_prop("pos"))])),
            s_prop_assign(e_this(), "pos", e_binop(e_var("end"), BinOp::Add, e_int(1))),
            s_return(e_var("value")),
        ])
}

/// `skipSpaces` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m70() -> MethodBuilder {
    method("skipSpaces")
        .private()
        .returns(TypeExpr::Void)
        .body(vec![
            s_while(e_binop(e_binop(e_this_prop("pos"), BinOp::Lt, e_this_prop("len")), BinOp::And, e_self_call("isSpace", vec![e_index(e_this_prop("src"), e_this_prop("pos"))])), vec![
                s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
            ]),
        ])
}

/// `readTag` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m71() -> MethodBuilder {
    method("readTag")
        .private()
        .param("isStart", TypeExpr::Bool)
        .returns(TypeExpr::Void)
        .body(vec![
            s_assign("name", e_str("")),
            s_while(e_binop(e_this_prop("pos"), BinOp::Lt, e_this_prop("len")), vec![
                s_assign("c", e_index(e_this_prop("src"), e_this_prop("pos"))),
                s_if(
                    e_binop(e_binop(e_self_call("isSpace", vec![e_var("c")]), BinOp::Or, e_binop(e_var("c"), BinOp::StrictEq, e_str("/"))), BinOp::Or, e_binop(e_var("c"), BinOp::StrictEq, e_str(">"))),
                    vec![
                        s_break(1),
                    ],
                    vec![],
                    None,
                ),
                s_assign("name", e_binop(e_var("name"), BinOp::Concat, e_var("c"))),
                s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
            ]),
            s_assign("name", e_call("strtolower", vec![e_call("str_replace", vec![e_str("\0"), e_str("�"), e_var("name")])])),
            s_assign("attrs", e_array(vec![])),
            s_assign("seen", e_array(vec![])),
            s_assign("selfClosing", e_bool(false)),
            s_while(e_bool(true), vec![
                s_expr(e_method_call(e_this(), "skipSpaces", vec![])),
                s_if(
                    e_binop(e_this_prop("pos"), BinOp::GtEq, e_this_prop("len")),
                    vec![
                        s_return_void(),
                    ],
                    vec![],
                    None,
                ),
                s_assign("c", e_index(e_this_prop("src"), e_this_prop("pos"))),
                s_if(
                    e_binop(e_var("c"), BinOp::StrictEq, e_str(">")),
                    vec![
                        s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                        s_break(1),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_var("c"), BinOp::StrictEq, e_str("/")),
                    vec![
                        s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                        s_if(
                            e_binop(e_method_call(e_this(), "peek", vec![]), BinOp::StrictEq, e_str(">")),
                            vec![
                                s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                                s_assign("selfClosing", e_bool(true)),
                                s_break(1),
                            ],
                            vec![],
                            None,
                        ),
                        s_continue(1),
                    ],
                    vec![],
                    None,
                ),
                s_assign("attrName", e_var("c")),
                s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                s_while(e_binop(e_this_prop("pos"), BinOp::Lt, e_this_prop("len")), vec![
                    s_assign("d", e_index(e_this_prop("src"), e_this_prop("pos"))),
                    s_if(
                        e_binop(e_binop(e_binop(e_self_call("isSpace", vec![e_var("d")]), BinOp::Or, e_binop(e_var("d"), BinOp::StrictEq, e_str("/"))), BinOp::Or, e_binop(e_var("d"), BinOp::StrictEq, e_str(">"))), BinOp::Or, e_binop(e_var("d"), BinOp::StrictEq, e_str("="))),
                        vec![
                            s_break(1),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("attrName", e_binop(e_var("attrName"), BinOp::Concat, e_var("d"))),
                    s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                ]),
                s_assign("attrName", e_call("strtolower", vec![e_call("str_replace", vec![e_str("\0"), e_str("�"), e_var("attrName")])])),
                s_expr(e_method_call(e_this(), "skipSpaces", vec![])),
                s_assign("value", e_str("")),
                s_if(
                    e_binop(e_method_call(e_this(), "peek", vec![]), BinOp::StrictEq, e_str("=")),
                    vec![
                        s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                        s_expr(e_method_call(e_this(), "skipSpaces", vec![])),
                        s_assign("q", e_method_call(e_this(), "peek", vec![])),
                        s_if(
                            e_binop(e_binop(e_var("q"), BinOp::StrictEq, e_str("\"")), BinOp::Or, e_binop(e_var("q"), BinOp::StrictEq, e_str("'"))),
                            vec![
                                s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                                s_assign("value", e_method_call(e_this(), "attrValueQuoted", vec![e_var("q")])),
                                s_if(
                                    e_binop(e_binop(e_this_prop("pos"), BinOp::GtEq, e_this_prop("len")), BinOp::And, e_this_prop("unterminated")),
                                    vec![
                                        s_return_void(),
                                    ],
                                    vec![],
                                    None,
                                ),
                            ],
                            vec![],
                            Some(vec![
                            s_assign("value", e_method_call(e_this(), "attrValueUnquoted", vec![])),
                        ]),
                        ),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_not(e_call("isset", vec![e_index(e_var("seen"), e_var("attrName"))])),
                    vec![
                        s_array_assign("seen", e_var("attrName"), e_bool(true)),
                        s_array_push("attrs", e_array(vec![e_var("attrName"), e_var("value")])),
                    ],
                    vec![],
                    None,
                ),
            ]),
            s_if(
                e_var("isStart"),
                vec![
                    s_expr(e_method_call(e_this(), "tbStart", vec![e_var("name"), e_var("attrs"), e_var("selfClosing")])),
                ],
                vec![],
                Some(vec![
                s_expr(e_method_call(e_this(), "tbEnd", vec![e_var("name")])),
            ]),
            ),
        ])
}

/// `attrValueQuoted` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m72() -> MethodBuilder {
    method("attrValueQuoted")
        .private()
        .param("quote", TypeExpr::Str)
        .returns(TypeExpr::Str)
        .body(vec![
            s_assign("value", e_str("")),
            s_prop_assign(e_this(), "unterminated", e_bool(false)),
            s_while(e_binop(e_this_prop("pos"), BinOp::Lt, e_this_prop("len")), vec![
                s_assign("c", e_index(e_this_prop("src"), e_this_prop("pos"))),
                s_if(
                    e_binop(e_var("c"), BinOp::StrictEq, e_var("quote")),
                    vec![
                        s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                        s_return(e_var("value")),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_var("c"), BinOp::StrictEq, e_str("&")),
                    vec![
                        s_assign("value", e_binop(e_var("value"), BinOp::Concat, e_method_call(e_this(), "charRef", vec![e_bool(true)]))),
                        s_continue(1),
                    ],
                    vec![],
                    None,
                ),
                s_assign("stop", e_call("strpos", vec![e_this_prop("src"), e_var("quote"), e_this_prop("pos")])),
                s_assign("amp", e_call("strpos", vec![e_this_prop("src"), e_str("&"), e_this_prop("pos")])),
                s_assign("end", e_ternary(e_binop(e_var("stop"), BinOp::StrictEq, e_bool(false)), e_this_prop("len"), e_var("stop"))),
                s_if(
                    e_binop(e_binop(e_var("amp"), BinOp::StrictNotEq, e_bool(false)), BinOp::And, e_binop(e_var("amp"), BinOp::Lt, e_var("end"))),
                    vec![
                        s_assign("end", e_var("amp")),
                    ],
                    vec![],
                    None,
                ),
                s_assign("value", e_binop(e_var("value"), BinOp::Concat, e_call("str_replace", vec![e_str("\0"), e_str("�"), e_call("substr", vec![e_this_prop("src"), e_this_prop("pos"), e_binop(e_var("end"), BinOp::Sub, e_this_prop("pos"))])]))),
                s_prop_assign(e_this(), "pos", e_var("end")),
            ]),
            s_prop_assign(e_this(), "unterminated", e_bool(true)),
            s_return(e_var("value")),
        ])
}

/// `attrValueUnquoted` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m73() -> MethodBuilder {
    method("attrValueUnquoted")
        .private()
        .returns(TypeExpr::Str)
        .body(vec![
            s_assign("value", e_str("")),
            s_while(e_binop(e_this_prop("pos"), BinOp::Lt, e_this_prop("len")), vec![
                s_assign("c", e_index(e_this_prop("src"), e_this_prop("pos"))),
                s_if(
                    e_binop(e_self_call("isSpace", vec![e_var("c")]), BinOp::Or, e_binop(e_var("c"), BinOp::StrictEq, e_str(">"))),
                    vec![
                        s_break(1),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_var("c"), BinOp::StrictEq, e_str("&")),
                    vec![
                        s_assign("value", e_binop(e_var("value"), BinOp::Concat, e_method_call(e_this(), "charRef", vec![e_bool(true)]))),
                        s_continue(1),
                    ],
                    vec![],
                    None,
                ),
                s_assign("value", e_binop(e_var("value"), BinOp::Concat, e_ternary(e_binop(e_var("c"), BinOp::StrictEq, e_str("\0")), e_str("�"), e_var("c")))),
                s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
            ]),
            s_return(e_var("value")),
        ])
}

/// `charRef` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m74() -> MethodBuilder {
    method("charRef")
        .private()
        .param("inAttribute", TypeExpr::Bool)
        .returns(TypeExpr::Str)
        .body(vec![
            s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
            s_assign("c", e_method_call(e_this(), "peek", vec![])),
            s_if(
                e_binop(e_var("c"), BinOp::StrictEq, e_str("#")),
                vec![
                    s_assign("hex", e_bool(false)),
                    s_assign("i", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                    s_if(
                        e_binop(e_binop(e_var("i"), BinOp::Lt, e_this_prop("len")), BinOp::And, e_binop(e_binop(e_index(e_this_prop("src"), e_var("i")), BinOp::StrictEq, e_str("x")), BinOp::Or, e_binop(e_index(e_this_prop("src"), e_var("i")), BinOp::StrictEq, e_str("X")))),
                        vec![
                            s_assign("hex", e_bool(true)),
                            s_expr(e_post_inc("i")),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("digitsStart", e_var("i")),
                    s_while(e_binop(e_var("i"), BinOp::Lt, e_this_prop("len")), vec![
                        s_assign("d", e_index(e_this_prop("src"), e_var("i"))),
                        s_if(
                            e_binop(e_binop(e_binop(e_var("d"), BinOp::GtEq, e_str("0")), BinOp::And, e_binop(e_var("d"), BinOp::LtEq, e_str("9"))), BinOp::Or, e_binop(e_var("hex"), BinOp::And, e_binop(e_binop(e_binop(e_var("d"), BinOp::GtEq, e_str("a")), BinOp::And, e_binop(e_var("d"), BinOp::LtEq, e_str("f"))), BinOp::Or, e_binop(e_binop(e_var("d"), BinOp::GtEq, e_str("A")), BinOp::And, e_binop(e_var("d"), BinOp::LtEq, e_str("F")))))),
                            vec![
                                s_expr(e_post_inc("i")),
                            ],
                            vec![],
                            Some(vec![
                            s_break(1),
                        ]),
                        ),
                    ]),
                    s_if(
                        e_binop(e_var("i"), BinOp::StrictEq, e_var("digitsStart")),
                        vec![
                            s_return(e_str("&")),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("digits", e_call("substr", vec![e_this_prop("src"), e_var("digitsStart"), e_binop(e_var("i"), BinOp::Sub, e_var("digitsStart"))])),
                    s_if(
                        e_binop(e_binop(e_var("i"), BinOp::Lt, e_this_prop("len")), BinOp::And, e_binop(e_index(e_this_prop("src"), e_var("i")), BinOp::StrictEq, e_str(";"))),
                        vec![
                            s_expr(e_post_inc("i")),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "pos", e_var("i")),
                    s_if(
                        e_binop(e_call("strlen", vec![e_var("digits")]), BinOp::Gt, e_int(8)),
                        vec![
                            s_return(e_str("�")),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("code", e_int(0)),
                    s_if(
                        e_var("hex"),
                        vec![
                            s_assign("code", e_cast(CastType::Int, e_call("hexdec", vec![e_var("digits")]))),
                        ],
                        vec![],
                        Some(vec![
                        s_assign("code", e_cast(CastType::Int, e_var("digits"))),
                    ]),
                    ),
                    s_return(e_self_call("codepointText", vec![e_var("code")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_not(e_self_call("isAlnum", vec![e_var("c")])),
                vec![
                    s_return(e_str("&")),
                ],
                vec![],
                None,
            ),
            s_assign("i", e_this_prop("pos")),
            s_assign("name", e_str("")),
            s_while(e_binop(e_binop(e_binop(e_var("i"), BinOp::Lt, e_this_prop("len")), BinOp::And, e_self_call("isAlnum", vec![e_index(e_this_prop("src"), e_var("i"))])), BinOp::And, e_binop(e_call("strlen", vec![e_var("name")]), BinOp::Lt, e_int(40))), vec![
                s_assign("name", e_binop(e_var("name"), BinOp::Concat, e_index(e_this_prop("src"), e_var("i")))),
                s_expr(e_post_inc("i")),
            ]),
            s_if(
                e_not(e_this_prop("entitiesLoaded")),
                vec![
                    s_prop_assign(e_this(), "entityTable", e_call("__elephc_dom_entity_table", vec![])),
                    s_prop_assign(e_this(), "legacyEntities", e_call("__elephc_dom_legacy_entities", vec![])),
                    s_prop_assign(e_this(), "entitiesLoaded", e_bool(true)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_binop(e_var("i"), BinOp::Lt, e_this_prop("len")), BinOp::And, e_binop(e_index(e_this_prop("src"), e_var("i")), BinOp::StrictEq, e_str(";"))), BinOp::And, e_call("isset", vec![e_index(e_this_prop("entityTable"), e_binop(e_var("name"), BinOp::Concat, e_str(";")))])),
                vec![
                    s_prop_assign(e_this(), "pos", e_binop(e_var("i"), BinOp::Add, e_int(1))),
                    s_return(e_index(e_this_prop("entityTable"), e_binop(e_var("name"), BinOp::Concat, e_str(";")))),
                ],
                vec![],
                None,
            ),
            s_for(Some(s_assign("n", e_call("strlen", vec![e_var("name")]))), Some(e_binop(e_var("n"), BinOp::GtEq, e_int(2))), Some(s_expr(e_post_dec("n"))), vec![
                s_assign("try", e_call("substr", vec![e_var("name"), e_int(0), e_var("n")])),
                s_if(
                    e_call("in_array", vec![e_var("try"), e_this_prop("legacyEntities"), e_bool(true)]),
                    vec![
                        s_assign("after", e_binop(e_this_prop("pos"), BinOp::Add, e_var("n"))),
                        s_assign("nextChar", e_ternary(e_binop(e_var("after"), BinOp::Lt, e_this_prop("len")), e_index(e_this_prop("src"), e_var("after")), e_str(""))),
                        s_if(
                            e_binop(e_var("inAttribute"), BinOp::And, e_binop(e_binop(e_var("nextChar"), BinOp::StrictEq, e_str("=")), BinOp::Or, e_self_call("isAlnum", vec![e_var("nextChar")]))),
                            vec![
                                s_return(e_str("&")),
                            ],
                            vec![],
                            None,
                        ),
                        s_prop_assign(e_this(), "pos", e_var("after")),
                        s_return(e_index(e_this_prop("entityTable"), e_binop(e_var("try"), BinOp::Concat, e_str(";")))),
                    ],
                    vec![],
                    None,
                ),
            ]),
            s_return(e_str("&")),
        ])
}

/// `isAlnum` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m75() -> MethodBuilder {
    method("isAlnum")
        .private()
        .static_()
        .param("c", TypeExpr::Str)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_binop(e_var("c"), BinOp::StrictEq, e_str("")),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_return(e_binop(e_binop(e_binop(e_binop(e_var("c"), BinOp::GtEq, e_str("0")), BinOp::And, e_binop(e_var("c"), BinOp::LtEq, e_str("9"))), BinOp::Or, e_binop(e_binop(e_var("c"), BinOp::GtEq, e_str("a")), BinOp::And, e_binop(e_var("c"), BinOp::LtEq, e_str("z")))), BinOp::Or, e_binop(e_binop(e_var("c"), BinOp::GtEq, e_str("A")), BinOp::And, e_binop(e_var("c"), BinOp::LtEq, e_str("Z"))))),
        ])
}

/// `codepointText` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m76() -> MethodBuilder {
    method("codepointText")
        .private()
        .static_()
        .param("code", TypeExpr::Int)
        .returns(TypeExpr::Str)
        .body(vec![
            s_if(
                e_binop(e_binop(e_binop(e_var("code"), BinOp::StrictEq, e_int(0)), BinOp::Or, e_binop(e_var("code"), BinOp::Gt, e_int(1114111))), BinOp::Or, e_binop(e_binop(e_var("code"), BinOp::GtEq, e_int(55296)), BinOp::And, e_binop(e_var("code"), BinOp::LtEq, e_int(57343)))),
                vec![
                    s_return(e_str("�")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_var("code"), BinOp::GtEq, e_int(128)), BinOp::And, e_binop(e_var("code"), BinOp::LtEq, e_int(159))),
                vec![
                    s_assign("map", e_array_assoc(vec![(e_int(128), e_int(8364)), (e_int(130), e_int(8218)), (e_int(131), e_int(402)), (e_int(132), e_int(8222)), (e_int(133), e_int(8230)), (e_int(134), e_int(8224)), (e_int(135), e_int(8225)), (e_int(136), e_int(710)), (e_int(137), e_int(8240)), (e_int(138), e_int(352)), (e_int(139), e_int(8249)), (e_int(140), e_int(338)), (e_int(142), e_int(381)), (e_int(145), e_int(8216)), (e_int(146), e_int(8217)), (e_int(147), e_int(8220)), (e_int(148), e_int(8221)), (e_int(149), e_int(8226)), (e_int(150), e_int(8211)), (e_int(151), e_int(8212)), (e_int(152), e_int(732)), (e_int(153), e_int(8482)), (e_int(154), e_int(353)), (e_int(155), e_int(8250)), (e_int(156), e_int(339)), (e_int(158), e_int(382)), (e_int(159), e_int(376))])),
                    s_if(
                        e_call("isset", vec![e_index(e_var("map"), e_var("code"))]),
                        vec![
                            s_assign("code", e_cast(CastType::Int, e_index(e_var("map"), e_var("code")))),
                        ],
                        vec![],
                        None,
                    ),
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
}

/// `scanRaw` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m77() -> MethodBuilder {
    method("scanRaw")
        .private()
        .returns(TypeExpr::Void)
        .body(vec![
            s_assign("tag", e_this_prop("rawTag")),
            s_assign("mode", e_this_prop("rawMode")),
            s_if(
                e_binop(e_var("mode"), BinOp::StrictEq, e_str("plaintext")),
                vec![
                    s_expr(e_method_call(e_this(), "emitChars", vec![e_call("substr", vec![e_this_prop("src"), e_this_prop("pos")])])),
                    s_prop_assign(e_this(), "pos", e_this_prop("len")),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_assign("from", e_this_prop("pos")),
            s_assign("limit", e_this_prop("len")),
            s_assign("found", e_neg(e_int(1))),
            s_assign("i", e_var("from")),
            s_while(e_bool(true), vec![
                s_assign("at", e_call("stripos", vec![e_this_prop("src"), e_binop(e_str("</"), BinOp::Concat, e_var("tag")), e_var("i")])),
                s_if(
                    e_binop(e_var("at"), BinOp::StrictEq, e_bool(false)),
                    vec![
                        s_break(1),
                    ],
                    vec![],
                    None,
                ),
                s_assign("after", e_binop(e_binop(e_var("at"), BinOp::Add, e_int(2)), BinOp::Add, e_call("strlen", vec![e_var("tag")]))),
                s_assign("c", e_ternary(e_binop(e_var("after"), BinOp::Lt, e_this_prop("len")), e_index(e_this_prop("src"), e_var("after")), e_str(""))),
                s_if(
                    e_binop(e_binop(e_binop(e_binop(e_var("c"), BinOp::StrictEq, e_str("")), BinOp::Or, e_self_call("isSpace", vec![e_var("c")])), BinOp::Or, e_binop(e_var("c"), BinOp::StrictEq, e_str("/"))), BinOp::Or, e_binop(e_var("c"), BinOp::StrictEq, e_str(">"))),
                    vec![
                        s_assign("found", e_var("at")),
                        s_break(1),
                    ],
                    vec![],
                    None,
                ),
                s_assign("i", e_binop(e_var("at"), BinOp::Add, e_int(1))),
            ]),
            s_assign("end", e_ternary(e_binop(e_var("found"), BinOp::StrictEq, e_neg(e_int(1))), e_this_prop("len"), e_var("found"))),
            s_assign("text", e_call("substr", vec![e_this_prop("src"), e_var("from"), e_binop(e_var("end"), BinOp::Sub, e_var("from"))])),
            s_if(
                e_binop(e_var("text"), BinOp::StrictNotEq, e_str("")),
                vec![
                    s_if(
                        e_binop(e_var("mode"), BinOp::StrictEq, e_str("rcdata")),
                        vec![
                            s_expr(e_method_call(e_this(), "emitChars", vec![e_method_call(e_this(), "decodeInText", vec![e_var("text")])])),
                        ],
                        vec![],
                        Some(vec![
                        s_expr(e_method_call(e_this(), "emitChars", vec![e_call("str_replace", vec![e_str("\0"), e_str("�"), e_var("text")])])),
                    ]),
                    ),
                ],
                vec![],
                None,
            ),
            s_prop_assign(e_this(), "pos", e_var("end")),
            s_if(
                e_binop(e_var("found"), BinOp::StrictEq, e_neg(e_int(1))),
                vec![
                    s_prop_assign(e_this(), "rawTag", e_str("")),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_prop_assign(e_this(), "rawTag", e_str("")),
            s_prop_assign(e_this(), "pos", e_binop(e_var("found"), BinOp::Add, e_int(2))),
            s_expr(e_method_call(e_this(), "readTag", vec![e_bool(false)])),
        ])
}

/// `decodeInText` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m78() -> MethodBuilder {
    method("decodeInText")
        .private()
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
            s_assign("savedSrc", e_this_prop("src")),
            s_assign("savedPos", e_this_prop("pos")),
            s_assign("savedLen", e_this_prop("len")),
            s_prop_assign(e_this(), "src", e_var("text")),
            s_prop_assign(e_this(), "len", e_call("strlen", vec![e_var("text")])),
            s_prop_assign(e_this(), "pos", e_int(0)),
            s_assign("out", e_str("")),
            s_while(e_binop(e_this_prop("pos"), BinOp::Lt, e_this_prop("len")), vec![
                s_assign("amp", e_call("strpos", vec![e_this_prop("src"), e_str("&"), e_this_prop("pos")])),
                s_if(
                    e_binop(e_var("amp"), BinOp::StrictEq, e_bool(false)),
                    vec![
                        s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_call("substr", vec![e_this_prop("src"), e_this_prop("pos")]))),
                        s_break(1),
                    ],
                    vec![],
                    None,
                ),
                s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_call("substr", vec![e_this_prop("src"), e_this_prop("pos"), e_binop(e_var("amp"), BinOp::Sub, e_this_prop("pos"))]))),
                s_prop_assign(e_this(), "pos", e_var("amp")),
                s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_method_call(e_this(), "charRef", vec![e_bool(false)]))),
            ]),
            s_prop_assign(e_this(), "src", e_var("savedSrc")),
            s_prop_assign(e_this(), "pos", e_var("savedPos")),
            s_prop_assign(e_this(), "len", e_var("savedLen")),
            s_return(e_var("out")),
        ])
}

/// `emitChars` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m79() -> MethodBuilder {
    method("emitChars")
        .private()
        .param("text", TypeExpr::Str)
        .returns(TypeExpr::Void)
        .body(vec![
            s_if(
                e_binop(e_var("text"), BinOp::StrictEq, e_str("")),
                vec![
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_expr(e_method_call(e_this(), "tbChars", vec![e_var("text")])),
        ])
}

/// `emitComment` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m80() -> MethodBuilder {
    method("emitComment")
        .private()
        .param("text", TypeExpr::Str)
        .returns(TypeExpr::Void)
        .body(vec![
            s_expr(e_method_call(e_this(), "flushText", vec![])),
            s_expr(e_method_call(e_this(), "tbComment", vec![e_var("text")])),
        ])
}

/// `emitDoctype` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m81() -> MethodBuilder {
    method("emitDoctype")
        .private()
        .param("name", TypeExpr::Str)
        .param("public", t_nullable(TypeExpr::Str))
        .param("system", t_nullable(TypeExpr::Str))
        .param("forceQuirks", TypeExpr::Bool)
        .returns(TypeExpr::Void)
        .body(vec![
            s_expr(e_method_call(e_this(), "flushText", vec![])),
            s_expr(e_method_call(e_this(), "tbDoctype", vec![e_var("name"), e_var("public"), e_var("system"), e_var("forceQuirks")])),
        ])
}

/// `curr` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m82() -> MethodBuilder {
    method("curr")
        .private()
        .returns(t_nullable(t_class("\\Dom\\Element")))
        .body(vec![
            s_assign("n", e_call("count", vec![e_this_prop("open")])),
            s_return(e_ternary(e_binop(e_var("n"), BinOp::Gt, e_int(0)), e_index(e_this_prop("open"), e_binop(e_var("n"), BinOp::Sub, e_int(1))), e_null())),
        ])
}

/// `currName` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m83() -> MethodBuilder {
    method("currName")
        .private()
        .returns(TypeExpr::Str)
        .body(vec![
            s_assign("c", e_method_call(e_this(), "curr", vec![])),
            s_return(e_ternary(e_binop(e_var("c"), BinOp::StrictEq, e_null()), e_str(""), e_prop(e_var("c"), "localName"))),
        ])
}

/// `pushOpen` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m84() -> MethodBuilder {
    method("pushOpen")
        .private()
        .param("el", t_class("\\Dom\\Element"))
        .returns(TypeExpr::Void)
        .body(vec![
            s_prop_array_push(e_this(), "open", e_var("el")),
        ])
}

/// `popOpen` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m85() -> MethodBuilder {
    method("popOpen")
        .private()
        .returns(t_nullable(t_class("\\Dom\\Element")))
        .body(vec![
            s_if(
                e_binop(e_call("count", vec![e_this_prop("open")]), BinOp::StrictEq, e_int(0)),
                vec![
                    s_return(e_null()),
                ],
                vec![],
                None,
            ),
            s_return(e_call("array_pop", vec![e_this_prop("open")])),
        ])
}

/// `stackHas` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m86() -> MethodBuilder {
    method("stackHas")
        .private()
        .param("name", TypeExpr::Str)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_foreach(e_this_prop("open"), None, "el", vec![
                s_if(
                    e_binop(e_prop(e_var("el"), "localName"), BinOp::StrictEq, e_var("name")),
                    vec![
                        s_return(e_bool(true)),
                    ],
                    vec![],
                    None,
                ),
            ]),
            s_return(e_bool(false)),
        ])
}

/// `openIndex` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m87() -> MethodBuilder {
    method("openIndex")
        .private()
        .param("el", t_class("\\Dom\\Element"))
        .returns(TypeExpr::Int)
        .body(vec![
            s_for(Some(s_assign("i", e_binop(e_call("count", vec![e_this_prop("open")]), BinOp::Sub, e_int(1)))), Some(e_binop(e_var("i"), BinOp::GtEq, e_int(0))), Some(s_expr(e_post_dec("i"))), vec![
                s_if(
                    e_binop(e_index(e_this_prop("open"), e_var("i")), BinOp::StrictEq, e_var("el")),
                    vec![
                        s_return(e_var("i")),
                    ],
                    vec![],
                    None,
                ),
            ]),
            s_return(e_neg(e_int(1))),
        ])
}

/// `removeOpen` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m88() -> MethodBuilder {
    method("removeOpen")
        .private()
        .param("el", t_class("\\Dom\\Element"))
        .returns(TypeExpr::Void)
        .body(vec![
            s_assign("i", e_method_call(e_this(), "openIndex", vec![e_var("el")])),
            s_if(
                e_binop(e_var("i"), BinOp::GtEq, e_int(0)),
                vec![
                    s_expr(e_call("array_splice", vec![e_this_prop("open"), e_var("i"), e_int(1)])),
                ],
                vec![],
                None,
            ),
        ])
}

/// `makeElement` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m89() -> MethodBuilder {
    method("makeElement")
        .private()
        .param("name", TypeExpr::Str)
        .param("attrs", t_array())
        .param_default("ns", TypeExpr::Str, e_str(""))
        .returns(t_class("\\Dom\\Element"))
        .body(vec![
            s_assign("el", e_new_fq("Dom\\Element", vec![e_var("name")])),
            s_prop_assign(e_var("el"), "ownerDocument", e_this_prop("doc")),
            s_if(
                e_binop(e_var("ns"), BinOp::StrictNotEq, e_str("")),
                vec![
                    s_prop_assign(e_var("el"), "namespaceURI", e_var("ns")),
                    s_prop_assign(e_var("el"), "localName", e_var("name")),
                    s_prop_assign(e_var("el"), "tagName", e_var("name")),
                    s_prop_assign(e_var("el"), "nodeName", e_var("name")),
                ],
                vec![],
                None,
            ),
            s_foreach(e_var("attrs"), None, "pair", vec![
                s_expr(e_method_call(e_var("el"), "setAttribute", vec![e_index(e_var("pair"), e_int(0)), e_index(e_var("pair"), e_int(1))])),
            ]),
            s_return(e_var("el")),
        ])
}

/// `insertForeign` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m90() -> MethodBuilder {
    method("insertForeign")
        .private()
        .param("name", TypeExpr::Str)
        .param("attrs", t_array())
        .param("ns", TypeExpr::Str)
        .returns(t_class("\\Dom\\Element"))
        .body(vec![
            s_if(
                e_binop(e_var("ns"), BinOp::StrictEq, e_self_const("SVG")),
                vec![
                    s_assign("name", e_self_call("svgTagName", vec![e_var("name")])),
                    s_assign("attrs", e_self_call("adjustSvgAttributes", vec![e_var("attrs")])),
                ],
                vec![],
                Some(vec![
                s_assign("attrs", e_self_call("adjustMathAttributes", vec![e_var("attrs")])),
            ]),
            ),
            s_assign("el", e_method_call(e_this(), "makeElement", vec![e_var("name"), e_var("attrs"), e_var("ns")])),
            s_expr(e_method_call(e_this(), "insertNode", vec![e_var("el")])),
            s_expr(e_method_call(e_this(), "pushOpen", vec![e_var("el")])),
            s_return(e_var("el")),
        ])
}

/// `svgTagName` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m91() -> MethodBuilder {
    method("svgTagName")
        .private()
        .static_()
        .param("name", TypeExpr::Str)
        .returns(TypeExpr::Str)
        .body(vec![
            s_assign("map", e_array_assoc(vec![(e_str("altglyph"), e_str("altGlyph")), (e_str("altglyphdef"), e_str("altGlyphDef")), (e_str("altglyphitem"), e_str("altGlyphItem")), (e_str("animatecolor"), e_str("animateColor")), (e_str("animatemotion"), e_str("animateMotion")), (e_str("animatetransform"), e_str("animateTransform")), (e_str("clippath"), e_str("clipPath")), (e_str("feblend"), e_str("feBlend")), (e_str("fecolormatrix"), e_str("feColorMatrix")), (e_str("fecomponenttransfer"), e_str("feComponentTransfer")), (e_str("fecomposite"), e_str("feComposite")), (e_str("feconvolvematrix"), e_str("feConvolveMatrix")), (e_str("fediffuselighting"), e_str("feDiffuseLighting")), (e_str("fedisplacementmap"), e_str("feDisplacementMap")), (e_str("fedistantlight"), e_str("feDistantLight")), (e_str("fedropshadow"), e_str("feDropShadow")), (e_str("feflood"), e_str("feFlood")), (e_str("fefunca"), e_str("feFuncA")), (e_str("fefuncb"), e_str("feFuncB")), (e_str("fefuncg"), e_str("feFuncG")), (e_str("fefuncr"), e_str("feFuncR")), (e_str("fegaussianblur"), e_str("feGaussianBlur")), (e_str("feimage"), e_str("feImage")), (e_str("femerge"), e_str("feMerge")), (e_str("femergenode"), e_str("feMergeNode")), (e_str("femorphology"), e_str("feMorphology")), (e_str("feoffset"), e_str("feOffset")), (e_str("fepointlight"), e_str("fePointLight")), (e_str("fespecularlighting"), e_str("feSpecularLighting")), (e_str("fespotlight"), e_str("feSpotLight")), (e_str("fetile"), e_str("feTile")), (e_str("feturbulence"), e_str("feTurbulence")), (e_str("foreignobject"), e_str("foreignObject")), (e_str("glyphref"), e_str("glyphRef")), (e_str("lineargradient"), e_str("linearGradient")), (e_str("radialgradient"), e_str("radialGradient")), (e_str("textpath"), e_str("textPath"))])),
            s_return(e_null_coalesce(e_index(e_var("map"), e_var("name")), e_var("name"))),
        ])
}

/// `adjustSvgAttributes` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m92() -> MethodBuilder {
    method("adjustSvgAttributes")
        .private()
        .static_()
        .param("attrs", t_array())
        .returns(t_array())
        .body(vec![
            s_assign("map", e_array_assoc(vec![(e_str("attributename"), e_str("attributeName")), (e_str("attributetype"), e_str("attributeType")), (e_str("basefrequency"), e_str("baseFrequency")), (e_str("baseprofile"), e_str("baseProfile")), (e_str("calcmode"), e_str("calcMode")), (e_str("clippathunits"), e_str("clipPathUnits")), (e_str("diffuseconstant"), e_str("diffuseConstant")), (e_str("edgemode"), e_str("edgeMode")), (e_str("filterunits"), e_str("filterUnits")), (e_str("glyphref"), e_str("glyphRef")), (e_str("gradienttransform"), e_str("gradientTransform")), (e_str("gradientunits"), e_str("gradientUnits")), (e_str("kernelmatrix"), e_str("kernelMatrix")), (e_str("kernelunitlength"), e_str("kernelUnitLength")), (e_str("keypoints"), e_str("keyPoints")), (e_str("keysplines"), e_str("keySplines")), (e_str("keytimes"), e_str("keyTimes")), (e_str("lengthadjust"), e_str("lengthAdjust")), (e_str("limitingconeangle"), e_str("limitingConeAngle")), (e_str("markerheight"), e_str("markerHeight")), (e_str("markerunits"), e_str("markerUnits")), (e_str("markerwidth"), e_str("markerWidth")), (e_str("maskcontentunits"), e_str("maskContentUnits")), (e_str("maskunits"), e_str("maskUnits")), (e_str("numoctaves"), e_str("numOctaves")), (e_str("pathlength"), e_str("pathLength")), (e_str("patterncontentunits"), e_str("patternContentUnits")), (e_str("patterntransform"), e_str("patternTransform")), (e_str("patternunits"), e_str("patternUnits")), (e_str("pointsatx"), e_str("pointsAtX")), (e_str("pointsaty"), e_str("pointsAtY")), (e_str("pointsatz"), e_str("pointsAtZ")), (e_str("preservealpha"), e_str("preserveAlpha")), (e_str("preserveaspectratio"), e_str("preserveAspectRatio")), (e_str("primitiveunits"), e_str("primitiveUnits")), (e_str("refx"), e_str("refX")), (e_str("refy"), e_str("refY")), (e_str("repeatcount"), e_str("repeatCount")), (e_str("repeatdur"), e_str("repeatDur")), (e_str("requiredextensions"), e_str("requiredExtensions")), (e_str("requiredfeatures"), e_str("requiredFeatures")), (e_str("specularconstant"), e_str("specularConstant")), (e_str("specularexponent"), e_str("specularExponent")), (e_str("spreadmethod"), e_str("spreadMethod")), (e_str("startoffset"), e_str("startOffset")), (e_str("stddeviation"), e_str("stdDeviation")), (e_str("stitchtiles"), e_str("stitchTiles")), (e_str("surfacescale"), e_str("surfaceScale")), (e_str("systemlanguage"), e_str("systemLanguage")), (e_str("tablevalues"), e_str("tableValues")), (e_str("targetx"), e_str("targetX")), (e_str("targety"), e_str("targetY")), (e_str("textlength"), e_str("textLength")), (e_str("viewbox"), e_str("viewBox")), (e_str("viewtarget"), e_str("viewTarget")), (e_str("xchannelselector"), e_str("xChannelSelector")), (e_str("ychannelselector"), e_str("yChannelSelector")), (e_str("zoomandpan"), e_str("zoomAndPan"))])),
            s_assign("out", e_array(vec![])),
            s_foreach(e_var("attrs"), None, "pair", vec![
                s_assign("name", e_null_coalesce(e_index(e_var("map"), e_index(e_var("pair"), e_int(0))), e_index(e_var("pair"), e_int(0)))),
                s_array_push("out", e_array(vec![e_var("name"), e_index(e_var("pair"), e_int(1))])),
            ]),
            s_return(e_var("out")),
        ])
}

/// `adjustMathAttributes` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m93() -> MethodBuilder {
    method("adjustMathAttributes")
        .private()
        .static_()
        .param("attrs", t_array())
        .returns(t_array())
        .body(vec![
            s_assign("out", e_array(vec![])),
            s_foreach(e_var("attrs"), None, "pair", vec![
                s_assign("name", e_ternary(e_binop(e_index(e_var("pair"), e_int(0)), BinOp::StrictEq, e_str("definitionurl")), e_str("definitionURL"), e_index(e_var("pair"), e_int(0)))),
                s_array_push("out", e_array(vec![e_var("name"), e_index(e_var("pair"), e_int(1))])),
            ]),
            s_return(e_var("out")),
        ])
}

/// `inForeign` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m94() -> MethodBuilder {
    method("inForeign")
        .private()
        .param("kind", TypeExpr::Str)
        .param("name", TypeExpr::Str)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_assign("node", e_method_call(e_this(), "curr", vec![])),
            s_if(
                e_binop(e_binop(e_var("node"), BinOp::StrictEq, e_null()), BinOp::Or, e_binop(e_prop(e_var("node"), "namespaceURI"), BinOp::StrictEq, e_self_const("HTMLNS"))),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_assign("isMath", e_binop(e_prop(e_var("node"), "namespaceURI"), BinOp::StrictEq, e_self_const("MATHML"))),
            s_if(
                e_binop(e_var("isMath"), BinOp::And, e_call("in_array", vec![e_prop(e_var("node"), "localName"), e_array(vec![e_str("mi"), e_str("mo"), e_str("mn"), e_str("ms"), e_str("mtext")]), e_bool(true)])),
                vec![
                    s_if(
                        e_binop(e_binop(e_binop(e_var("kind"), BinOp::StrictEq, e_str("start")), BinOp::And, e_binop(e_var("name"), BinOp::StrictNotEq, e_str("mglyph"))), BinOp::And, e_binop(e_var("name"), BinOp::StrictNotEq, e_str("malignmark"))),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("kind"), BinOp::StrictEq, e_str("chars")),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_binop(e_var("isMath"), BinOp::And, e_binop(e_prop(e_var("node"), "localName"), BinOp::StrictEq, e_str("annotation-xml"))), BinOp::And, e_binop(e_var("kind"), BinOp::StrictEq, e_str("start"))), BinOp::And, e_binop(e_var("name"), BinOp::StrictEq, e_str("svg"))),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_method_call(e_this(), "isHtmlIntegration", vec![e_var("node")]), BinOp::And, e_binop(e_binop(e_var("kind"), BinOp::StrictEq, e_str("start")), BinOp::Or, e_binop(e_var("kind"), BinOp::StrictEq, e_str("chars")))),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_return(e_bool(true)),
        ])
}

/// `isHtmlIntegration` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m95() -> MethodBuilder {
    method("isHtmlIntegration")
        .private()
        .param("node", t_class("\\Dom\\Element"))
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_binop(e_prop(e_var("node"), "namespaceURI"), BinOp::StrictEq, e_self_const("SVG")),
                vec![
                    s_return(e_binop(e_binop(e_binop(e_prop(e_var("node"), "localName"), BinOp::StrictEq, e_str("foreignObject")), BinOp::Or, e_binop(e_prop(e_var("node"), "localName"), BinOp::StrictEq, e_str("desc"))), BinOp::Or, e_binop(e_prop(e_var("node"), "localName"), BinOp::StrictEq, e_str("title")))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_prop(e_var("node"), "namespaceURI"), BinOp::StrictEq, e_self_const("MATHML")), BinOp::And, e_binop(e_prop(e_var("node"), "localName"), BinOp::StrictEq, e_str("annotation-xml"))),
                vec![
                    s_assign("enc", e_method_call(e_var("node"), "getAttribute", vec![e_str("encoding")])),
                    s_if(
                        e_binop(e_var("enc"), BinOp::StrictNotEq, e_null()),
                        vec![
                            s_assign("enc", e_call("strtolower", vec![e_var("enc")])),
                            s_return(e_binop(e_binop(e_var("enc"), BinOp::StrictEq, e_str("text/html")), BinOp::Or, e_binop(e_var("enc"), BinOp::StrictEq, e_str("application/xhtml+xml")))),
                        ],
                        vec![],
                        None,
                    ),
                ],
                vec![],
                None,
            ),
            s_return(e_bool(false)),
        ])
}

/// `foreignStart` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m96() -> MethodBuilder {
    method("foreignStart")
        .private()
        .param("name", TypeExpr::Str)
        .param("attrs", t_array())
        .param("selfClosing", TypeExpr::Bool)
        .returns(TypeExpr::Void)
        .body(vec![
            s_assign("breakout", e_array(vec![e_str("b"), e_str("big"), e_str("blockquote"), e_str("body"), e_str("br"), e_str("center"), e_str("code"), e_str("dd"), e_str("div"), e_str("dl"), e_str("dt"), e_str("em"), e_str("embed"), e_str("h1"), e_str("h2"), e_str("h3"), e_str("h4"), e_str("h5"), e_str("h6"), e_str("head"), e_str("hr"), e_str("i"), e_str("img"), e_str("li"), e_str("listing"), e_str("menu"), e_str("meta"), e_str("nobr"), e_str("ol"), e_str("p"), e_str("pre"), e_str("ruby"), e_str("s"), e_str("small"), e_str("span"), e_str("strong"), e_str("strike"), e_str("sub"), e_str("table"), e_str("tt"), e_str("u"), e_str("ul"), e_str("var")])),
            s_assign("isBreakout", e_call("in_array", vec![e_var("name"), e_var("breakout"), e_bool(true)])),
            s_if(
                e_binop(e_var("name"), BinOp::StrictEq, e_str("font")),
                vec![
                    s_foreach(e_var("attrs"), None, "pair", vec![
                        s_if(
                            e_binop(e_binop(e_binop(e_index(e_var("pair"), e_int(0)), BinOp::StrictEq, e_str("color")), BinOp::Or, e_binop(e_index(e_var("pair"), e_int(0)), BinOp::StrictEq, e_str("face"))), BinOp::Or, e_binop(e_index(e_var("pair"), e_int(0)), BinOp::StrictEq, e_str("size"))),
                            vec![
                                s_assign("isBreakout", e_bool(true)),
                            ],
                            vec![],
                            None,
                        ),
                    ]),
                ],
                vec![],
                None,
            ),
            s_if(
                e_var("isBreakout"),
                vec![
                    s_while(e_binop(e_call("count", vec![e_this_prop("open")]), BinOp::Gt, e_int(0)), vec![
                        s_assign("node", e_method_call(e_this(), "curr", vec![])),
                        s_if(
                            e_binop(e_binop(e_binop(e_prop(e_var("node"), "namespaceURI"), BinOp::StrictEq, e_self_const("HTMLNS")), BinOp::Or, e_method_call(e_this(), "isHtmlIntegration", vec![e_var("node")])), BinOp::Or, e_binop(e_binop(e_prop(e_var("node"), "namespaceURI"), BinOp::StrictEq, e_self_const("MATHML")), BinOp::And, e_call("in_array", vec![e_prop(e_var("node"), "localName"), e_array(vec![e_str("mi"), e_str("mo"), e_str("mn"), e_str("ms"), e_str("mtext")]), e_bool(true)]))),
                            vec![
                                s_break(1),
                            ],
                            vec![],
                            None,
                        ),
                        s_expr(e_method_call(e_this(), "popOpen", vec![])),
                    ]),
                    s_expr(e_method_call(e_this(), "dispatchStart", vec![e_var("name"), e_var("attrs"), e_var("selfClosing")])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_assign("ns", e_prop(e_method_call(e_this(), "curr", vec![]), "namespaceURI")),
            s_assign("el", e_method_call(e_this(), "insertForeign", vec![e_var("name"), e_var("attrs"), e_var("ns")])),
            s_if(
                e_var("selfClosing"),
                vec![
                    s_expr(e_method_call(e_this(), "popOpen", vec![])),
                ],
                vec![],
                None,
            ),
        ])
}

/// `foreignEnd` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m97() -> MethodBuilder {
    method("foreignEnd")
        .private()
        .param("name", TypeExpr::Str)
        .returns(TypeExpr::Void)
        .body(vec![
            s_if(
                e_binop(e_binop(e_var("name"), BinOp::StrictEq, e_str("p")), BinOp::Or, e_binop(e_var("name"), BinOp::StrictEq, e_str("br"))),
                vec![
                    s_while(e_binop(e_call("count", vec![e_this_prop("open")]), BinOp::Gt, e_int(0)), vec![
                        s_assign("top", e_method_call(e_this(), "curr", vec![])),
                        s_if(
                            e_binop(e_binop(e_binop(e_prop(e_var("top"), "namespaceURI"), BinOp::StrictEq, e_self_const("HTMLNS")), BinOp::Or, e_method_call(e_this(), "isHtmlIntegration", vec![e_var("top")])), BinOp::Or, e_binop(e_binop(e_prop(e_var("top"), "namespaceURI"), BinOp::StrictEq, e_self_const("MATHML")), BinOp::And, e_call("in_array", vec![e_prop(e_var("top"), "localName"), e_array(vec![e_str("mi"), e_str("mo"), e_str("mn"), e_str("ms"), e_str("mtext")]), e_bool(true)]))),
                            vec![
                                s_break(1),
                            ],
                            vec![],
                            None,
                        ),
                        s_expr(e_method_call(e_this(), "popOpen", vec![])),
                    ]),
                    s_expr(e_method_call(e_this(), "dispatchEnd", vec![e_var("name")])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_assign("i", e_binop(e_call("count", vec![e_this_prop("open")]), BinOp::Sub, e_int(1))),
            s_assign("node", e_index(e_this_prop("open"), e_var("i"))),
            s_if(
                e_binop(e_binop(e_binop(e_prop(e_var("node"), "namespaceURI"), BinOp::StrictEq, e_self_const("SVG")), BinOp::And, e_binop(e_prop(e_var("node"), "localName"), BinOp::StrictEq, e_str("script"))), BinOp::And, e_binop(e_var("name"), BinOp::StrictEq, e_str("script"))),
                vec![
                    s_expr(e_method_call(e_this(), "popOpen", vec![])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_while(e_bool(true), vec![
                s_if(
                    e_binop(e_var("i"), BinOp::StrictEq, e_int(0)),
                    vec![
                        s_return_void(),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_call("strtolower", vec![e_prop(e_var("node"), "localName")]), BinOp::StrictEq, e_var("name")),
                    vec![
                        s_while(e_binop(e_call("count", vec![e_this_prop("open")]), BinOp::Gt, e_var("i")), vec![
                            s_expr(e_call("array_pop", vec![e_this_prop("open")])),
                        ]),
                        s_return_void(),
                    ],
                    vec![],
                    None,
                ),
                s_expr(e_post_dec("i")),
                s_assign("node", e_index(e_this_prop("open"), e_var("i"))),
                s_if(
                    e_binop(e_prop(e_var("node"), "namespaceURI"), BinOp::StrictEq, e_self_const("HTMLNS")),
                    vec![
                        s_expr(e_method_call(e_this(), "dispatchEnd", vec![e_var("name")])),
                        s_return_void(),
                    ],
                    vec![],
                    None,
                ),
            ]),
        ])
}

/// `insertionParent` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m98() -> MethodBuilder {
    method("insertionParent")
        .private()
        .param("override", t_nullable(t_class("\\Dom\\Node")))
        .param_by_ref("before", Some(t_nullable(t_class("\\Dom\\Node"))))
        .returns(t_class("\\Dom\\Node"))
        .body(vec![
            s_assign("before", e_null()),
            s_assign("target", e_var("override")),
            s_if(
                e_binop(e_var("target"), BinOp::StrictEq, e_null()),
                vec![
                    s_assign("target", e_method_call(e_this(), "curr", vec![])),
                    s_if(
                        e_binop(e_var("target"), BinOp::StrictEq, e_null()),
                        vec![
                            s_assign("target", e_this_prop("doc")),
                        ],
                        vec![],
                        None,
                    ),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_this_prop("foster"), BinOp::And, e_instance_of(e_var("target"), "\\Dom\\Element")), BinOp::And, e_call("in_array", vec![e_prop(e_var("target"), "localName"), e_array(vec![e_str("table"), e_str("tbody"), e_str("tfoot"), e_str("thead"), e_str("tr")]), e_bool(true)])),
                vec![
                    s_assign("tableIndex", e_neg(e_int(1))),
                    s_for(Some(s_assign("i", e_binop(e_call("count", vec![e_this_prop("open")]), BinOp::Sub, e_int(1)))), Some(e_binop(e_var("i"), BinOp::GtEq, e_int(0))), Some(s_expr(e_post_dec("i"))), vec![
                        s_if(
                            e_binop(e_prop(e_index(e_this_prop("open"), e_var("i")), "localName"), BinOp::StrictEq, e_str("table")),
                            vec![
                                s_assign("tableIndex", e_var("i")),
                                s_break(1),
                            ],
                            vec![],
                            None,
                        ),
                    ]),
                    s_if(
                        e_binop(e_var("tableIndex"), BinOp::Lt, e_int(0)),
                        vec![
                            s_return(e_index(e_this_prop("open"), e_int(0))),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("table", e_index(e_this_prop("open"), e_var("tableIndex"))),
                    s_if(
                        e_binop(e_prop(e_var("table"), "parentNode"), BinOp::StrictNotEq, e_null()),
                        vec![
                            s_assign("before", e_var("table")),
                            s_return(e_prop(e_var("table"), "parentNode")),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_ternary(e_binop(e_var("tableIndex"), BinOp::Gt, e_int(0)), e_index(e_this_prop("open"), e_binop(e_var("tableIndex"), BinOp::Sub, e_int(1))), e_this_prop("doc"))),
                ],
                vec![],
                None,
            ),
            s_return(e_var("target")),
        ])
}

/// `insertNode` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m99() -> MethodBuilder {
    method("insertNode")
        .private()
        .param("node", t_class("\\Dom\\Node"))
        .param_default("override", t_nullable(t_class("\\Dom\\Node")), e_null())
        .returns(TypeExpr::Void)
        .body(vec![
            s_assign("before", e_null()),
            s_assign("parent", e_method_call(e_this(), "insertionParent", vec![e_var("override"), e_var("before")])),
            s_expr(e_method_call(e_var("parent"), "insertBefore", vec![e_var("node"), e_var("before")])),
        ])
}

/// `insertElement` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m100() -> MethodBuilder {
    method("insertElement")
        .private()
        .param("name", TypeExpr::Str)
        .param("attrs", t_array())
        .returns(t_class("\\Dom\\Element"))
        .body(vec![
            s_assign("el", e_method_call(e_this(), "makeElement", vec![e_var("name"), e_var("attrs")])),
            s_expr(e_method_call(e_this(), "insertNode", vec![e_var("el")])),
            s_expr(e_method_call(e_this(), "pushOpen", vec![e_var("el")])),
            s_return(e_var("el")),
        ])
}

/// `insertVoid` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m101() -> MethodBuilder {
    method("insertVoid")
        .private()
        .param("name", TypeExpr::Str)
        .param("attrs", t_array())
        .returns(t_class("\\Dom\\Element"))
        .body(vec![
            s_assign("el", e_method_call(e_this(), "makeElement", vec![e_var("name"), e_var("attrs")])),
            s_expr(e_method_call(e_this(), "insertNode", vec![e_var("el")])),
            s_return(e_var("el")),
        ])
}

/// `resolveTableText` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m102() -> MethodBuilder {
    method("resolveTableText")
        .private()
        .returns(TypeExpr::Void)
        .body(vec![
            s_assign("text", e_this_prop("tableTextBuf")),
            s_prop_assign(e_this(), "tableTextBuf", e_str("")),
            s_if(
                e_binop(e_var("text"), BinOp::StrictEq, e_str("")),
                vec![
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_call("trim", vec![e_var("text"), e_str(" \t\n\u{c}")]), BinOp::StrictEq, e_str("")),
                vec![
                    s_expr(e_method_call(e_this(), "insertCharsNow", vec![e_var("text")])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_assign("saved", e_this_prop("foster")),
            s_prop_assign(e_this(), "foster", e_bool(true)),
            s_expr(e_method_call(e_this(), "flushText", vec![])),
            s_expr(e_method_call(e_this(), "bodyChars", vec![e_var("text")])),
            s_expr(e_method_call(e_this(), "flushText", vec![])),
            s_prop_assign(e_this(), "foster", e_var("saved")),
        ])
}

/// `flushText` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m103() -> MethodBuilder {
    method("flushText")
        .private()
        .returns(TypeExpr::Void)
        .body(vec![
            s_if(
                e_binop(e_this_prop("tableTextBuf"), BinOp::StrictNotEq, e_str("")),
                vec![
                    s_expr(e_method_call(e_this(), "resolveTableText", vec![])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_this_prop("pendingText"), BinOp::StrictEq, e_str("")),
                vec![
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_assign("text", e_this_prop("pendingText")),
            s_prop_assign(e_this(), "pendingText", e_str("")),
            s_assign("before", e_null()),
            s_assign("parent", e_method_call(e_this(), "insertionParent", vec![e_null(), e_var("before")])),
            s_if(
                e_binop(e_var("before"), BinOp::StrictEq, e_null()),
                vec![
                    s_assign("last", e_prop(e_var("parent"), "lastChild")),
                ],
                vec![],
                Some(vec![
                s_assign("last", e_prop(e_var("before"), "previousSibling")),
            ]),
            ),
            s_if(
                e_instance_of(e_var("last"), "\\Dom\\Text"),
                vec![
                    s_expr(e_method_call(e_var("last"), "setData", vec![e_binop(e_prop(e_var("last"), "data"), BinOp::Concat, e_var("text"))])),
                    s_prop_assign(e_var("last"), "wholeText", e_prop(e_var("last"), "data")),
                    s_expr(e_method_call(e_var("last"), "refreshText", vec![])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_assign("node", e_new_fq("Dom\\Text", vec![e_var("text")])),
            s_prop_assign(e_var("node"), "ownerDocument", e_this_prop("doc")),
            s_expr(e_method_call(e_var("parent"), "insertBefore", vec![e_var("node"), e_var("before")])),
        ])
}

/// `insertCharsNow` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m104() -> MethodBuilder {
    method("insertCharsNow")
        .private()
        .param("text", TypeExpr::Str)
        .returns(TypeExpr::Void)
        .body(vec![
            s_prop_assign(e_this(), "pendingText", e_binop(e_this_prop("pendingText"), BinOp::Concat, e_var("text"))),
        ])
}

/// `inScope` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m105() -> MethodBuilder {
    method("inScope")
        .private()
        .param("name", TypeExpr::Str)
        .param_default("kind", TypeExpr::Str, e_str("default"))
        .returns(TypeExpr::Bool)
        .body(vec![
            s_for(Some(s_assign("i", e_binop(e_call("count", vec![e_this_prop("open")]), BinOp::Sub, e_int(1)))), Some(e_binop(e_var("i"), BinOp::GtEq, e_int(0))), Some(s_expr(e_post_dec("i"))), vec![
                s_assign("el", e_index(e_this_prop("open"), e_var("i"))),
                s_assign("n", e_prop(e_var("el"), "localName")),
                s_if(
                    e_binop(e_binop(e_var("n"), BinOp::StrictEq, e_var("name")), BinOp::And, e_binop(e_prop(e_var("el"), "namespaceURI"), BinOp::StrictEq, e_self_const("HTMLNS"))),
                    vec![
                        s_return(e_bool(true)),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_method_call(e_this(), "elementIsBoundary", vec![e_var("el"), e_var("kind")]),
                    vec![
                        s_return(e_bool(false)),
                    ],
                    vec![],
                    None,
                ),
            ]),
            s_return(e_bool(false)),
        ])
}

/// `elementIsBoundary` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m106() -> MethodBuilder {
    method("elementIsBoundary")
        .private()
        .param("el", t_class("\\Dom\\Element"))
        .param("kind", TypeExpr::Str)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_binop(e_prop(e_var("el"), "namespaceURI"), BinOp::StrictEq, e_self_const("HTMLNS")),
                vec![
                    s_return(e_method_call(e_this(), "scopeBoundary", vec![e_prop(e_var("el"), "localName"), e_var("kind")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_var("kind"), BinOp::StrictEq, e_str("table")), BinOp::Or, e_binop(e_var("kind"), BinOp::StrictEq, e_str("select"))),
                vec![
                    s_return(e_ternary(e_binop(e_var("kind"), BinOp::StrictEq, e_str("table")), e_bool(false), e_bool(true))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_prop(e_var("el"), "namespaceURI"), BinOp::StrictEq, e_self_const("MATHML")),
                vec![
                    s_return(e_call("in_array", vec![e_prop(e_var("el"), "localName"), e_array(vec![e_str("mi"), e_str("mo"), e_str("mn"), e_str("ms"), e_str("mtext"), e_str("annotation-xml")]), e_bool(true)])),
                ],
                vec![],
                None,
            ),
            s_return(e_binop(e_binop(e_binop(e_prop(e_var("el"), "localName"), BinOp::StrictEq, e_str("foreignObject")), BinOp::Or, e_binop(e_prop(e_var("el"), "localName"), BinOp::StrictEq, e_str("desc"))), BinOp::Or, e_binop(e_prop(e_var("el"), "localName"), BinOp::StrictEq, e_str("title")))),
        ])
}

/// `scopeBoundary` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m107() -> MethodBuilder {
    method("scopeBoundary")
        .private()
        .param("n", TypeExpr::Str)
        .param("kind", TypeExpr::Str)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_binop(e_var("kind"), BinOp::StrictEq, e_str("table")),
                vec![
                    s_return(e_binop(e_binop(e_binop(e_var("n"), BinOp::StrictEq, e_str("html")), BinOp::Or, e_binop(e_var("n"), BinOp::StrictEq, e_str("table"))), BinOp::Or, e_binop(e_var("n"), BinOp::StrictEq, e_str("template")))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("kind"), BinOp::StrictEq, e_str("select")),
                vec![
                    s_return(e_binop(e_binop(e_var("n"), BinOp::StrictNotEq, e_str("optgroup")), BinOp::And, e_binop(e_var("n"), BinOp::StrictNotEq, e_str("option")))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_call("in_array", vec![e_var("n"), e_array(vec![e_str("applet"), e_str("caption"), e_str("html"), e_str("table"), e_str("td"), e_str("th"), e_str("marquee"), e_str("object"), e_str("template")]), e_bool(true)]),
                vec![
                    s_return(e_bool(true)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_var("kind"), BinOp::StrictEq, e_str("list")), BinOp::And, e_binop(e_binop(e_var("n"), BinOp::StrictEq, e_str("ol")), BinOp::Or, e_binop(e_var("n"), BinOp::StrictEq, e_str("ul")))),
                vec![
                    s_return(e_bool(true)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_var("kind"), BinOp::StrictEq, e_str("button")), BinOp::And, e_binop(e_var("n"), BinOp::StrictEq, e_str("button"))),
                vec![
                    s_return(e_bool(true)),
                ],
                vec![],
                None,
            ),
            s_return(e_bool(false)),
        ])
}

/// `headingInScope` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m108() -> MethodBuilder {
    method("headingInScope")
        .private()
        .returns(TypeExpr::Bool)
        .body(vec![
            s_for(Some(s_assign("i", e_binop(e_call("count", vec![e_this_prop("open")]), BinOp::Sub, e_int(1)))), Some(e_binop(e_var("i"), BinOp::GtEq, e_int(0))), Some(s_expr(e_post_dec("i"))), vec![
                s_assign("el", e_index(e_this_prop("open"), e_var("i"))),
                s_assign("n", e_prop(e_var("el"), "localName")),
                s_if(
                    e_binop(e_binop(e_binop(e_binop(e_binop(e_call("strlen", vec![e_var("n")]), BinOp::StrictEq, e_int(2)), BinOp::And, e_binop(e_index(e_var("n"), e_int(0)), BinOp::StrictEq, e_str("h"))), BinOp::And, e_binop(e_index(e_var("n"), e_int(1)), BinOp::GtEq, e_str("1"))), BinOp::And, e_binop(e_index(e_var("n"), e_int(1)), BinOp::LtEq, e_str("6"))), BinOp::And, e_binop(e_prop(e_var("el"), "namespaceURI"), BinOp::StrictEq, e_self_const("HTMLNS"))),
                    vec![
                        s_return(e_bool(true)),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_method_call(e_this(), "elementIsBoundary", vec![e_var("el"), e_str("default")]),
                    vec![
                        s_return(e_bool(false)),
                    ],
                    vec![],
                    None,
                ),
            ]),
            s_return(e_bool(false)),
        ])
}

/// `generateImpliedEndTags` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m109() -> MethodBuilder {
    method("generateImpliedEndTags")
        .private()
        .param_default("except", TypeExpr::Str, e_str(""))
        .returns(TypeExpr::Void)
        .body(vec![
            s_while(e_bool(true), vec![
                s_assign("n", e_method_call(e_this(), "currName", vec![])),
                s_if(
                    e_binop(e_binop(e_var("n"), BinOp::StrictNotEq, e_var("except")), BinOp::And, e_call("in_array", vec![e_var("n"), e_array(vec![e_str("dd"), e_str("dt"), e_str("li"), e_str("optgroup"), e_str("option"), e_str("p"), e_str("rb"), e_str("rp"), e_str("rt"), e_str("rtc")]), e_bool(true)])),
                    vec![
                        s_expr(e_method_call(e_this(), "popOpen", vec![])),
                        s_continue(1),
                    ],
                    vec![],
                    None,
                ),
                s_break(1),
            ]),
        ])
}

/// `generateAllImpliedEndTags` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m110() -> MethodBuilder {
    method("generateAllImpliedEndTags")
        .private()
        .returns(TypeExpr::Void)
        .body(vec![
            s_while(e_call("in_array", vec![e_method_call(e_this(), "currName", vec![]), e_array(vec![e_str("caption"), e_str("colgroup"), e_str("dd"), e_str("dt"), e_str("li"), e_str("optgroup"), e_str("option"), e_str("p"), e_str("rb"), e_str("rp"), e_str("rt"), e_str("rtc"), e_str("tbody"), e_str("td"), e_str("tfoot"), e_str("th"), e_str("thead"), e_str("tr")]), e_bool(true)]), vec![
                s_expr(e_method_call(e_this(), "popOpen", vec![])),
            ]),
        ])
}

/// `closeP` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m111() -> MethodBuilder {
    method("closeP")
        .private()
        .returns(TypeExpr::Void)
        .body(vec![
            s_expr(e_method_call(e_this(), "generateImpliedEndTags", vec![e_str("p")])),
            s_while(e_binop(e_call("count", vec![e_this_prop("open")]), BinOp::Gt, e_int(0)), vec![
                s_assign("el", e_method_call(e_this(), "popOpen", vec![])),
                s_if(
                    e_binop(e_prop(e_var("el"), "localName"), BinOp::StrictEq, e_str("p")),
                    vec![
                        s_break(1),
                    ],
                    vec![],
                    None,
                ),
            ]),
        ])
}

/// `closePIfInButtonScope` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m112() -> MethodBuilder {
    method("closePIfInButtonScope")
        .private()
        .returns(TypeExpr::Void)
        .body(vec![
            s_if(
                e_method_call(e_this(), "inScope", vec![e_str("p"), e_str("button")]),
                vec![
                    s_expr(e_method_call(e_this(), "closeP", vec![])),
                ],
                vec![],
                None,
            ),
        ])
}

/// `popUntil` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m113() -> MethodBuilder {
    method("popUntil")
        .private()
        .param("name", TypeExpr::Str)
        .returns(TypeExpr::Void)
        .body(vec![
            s_while(e_binop(e_call("count", vec![e_this_prop("open")]), BinOp::Gt, e_int(0)), vec![
                s_assign("el", e_method_call(e_this(), "popOpen", vec![])),
                s_if(
                    e_binop(e_prop(e_var("el"), "localName"), BinOp::StrictEq, e_var("name")),
                    vec![
                        s_break(1),
                    ],
                    vec![],
                    None,
                ),
            ]),
        ])
}

/// `popUntilOneOf` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m114() -> MethodBuilder {
    method("popUntilOneOf")
        .private()
        .param("names", t_array())
        .returns(TypeExpr::Void)
        .body(vec![
            s_while(e_binop(e_call("count", vec![e_this_prop("open")]), BinOp::Gt, e_int(0)), vec![
                s_assign("el", e_method_call(e_this(), "popOpen", vec![])),
                s_if(
                    e_call("in_array", vec![e_prop(e_var("el"), "localName"), e_var("names"), e_bool(true)]),
                    vec![
                        s_break(1),
                    ],
                    vec![],
                    None,
                ),
            ]),
        ])
}

/// `isSpecial` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m115() -> MethodBuilder {
    method("isSpecial")
        .private()
        .static_()
        .param("n", TypeExpr::Str)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_call("in_array", vec![e_var("n"), e_array(vec![e_str("address"), e_str("applet"), e_str("area"), e_str("article"), e_str("aside"), e_str("base"), e_str("basefont"), e_str("bgsound"), e_str("blockquote"), e_str("body"), e_str("br"), e_str("button"), e_str("caption"), e_str("center"), e_str("col"), e_str("colgroup"), e_str("dd"), e_str("details"), e_str("dir"), e_str("div"), e_str("dl"), e_str("dt"), e_str("embed"), e_str("fieldset"), e_str("figcaption"), e_str("figure"), e_str("footer"), e_str("form"), e_str("frame"), e_str("frameset"), e_str("h1"), e_str("h2"), e_str("h3"), e_str("h4"), e_str("h5"), e_str("h6"), e_str("head"), e_str("header"), e_str("hgroup"), e_str("hr"), e_str("html"), e_str("iframe"), e_str("img"), e_str("input"), e_str("keygen"), e_str("li"), e_str("link"), e_str("listing"), e_str("main"), e_str("marquee"), e_str("menu"), e_str("meta"), e_str("nav"), e_str("noembed"), e_str("noframes"), e_str("noscript"), e_str("object"), e_str("ol"), e_str("p"), e_str("param"), e_str("plaintext"), e_str("pre"), e_str("script"), e_str("search"), e_str("section"), e_str("select"), e_str("source"), e_str("style"), e_str("summary"), e_str("table"), e_str("tbody"), e_str("td"), e_str("template"), e_str("textarea"), e_str("tfoot"), e_str("th"), e_str("thead"), e_str("title"), e_str("tr"), e_str("track"), e_str("ul"), e_str("wbr"), e_str("xmp")]), e_bool(true)])),
        ])
}

/// `isSpecialEl` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m116() -> MethodBuilder {
    method("isSpecialEl")
        .private()
        .param("el", t_class("\\Dom\\Element"))
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_binop(e_prop(e_var("el"), "namespaceURI"), BinOp::StrictEq, e_self_const("HTMLNS")),
                vec![
                    s_return(e_self_call("isSpecial", vec![e_prop(e_var("el"), "localName")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_prop(e_var("el"), "namespaceURI"), BinOp::StrictEq, e_self_const("MATHML")),
                vec![
                    s_return(e_call("in_array", vec![e_prop(e_var("el"), "localName"), e_array(vec![e_str("mi"), e_str("mo"), e_str("mn"), e_str("ms"), e_str("mtext"), e_str("annotation-xml")]), e_bool(true)])),
                ],
                vec![],
                None,
            ),
            s_return(e_binop(e_binop(e_binop(e_prop(e_var("el"), "localName"), BinOp::StrictEq, e_str("foreignObject")), BinOp::Or, e_binop(e_prop(e_var("el"), "localName"), BinOp::StrictEq, e_str("desc"))), BinOp::Or, e_binop(e_prop(e_var("el"), "localName"), BinOp::StrictEq, e_str("title")))),
        ])
}

/// `isFormatting` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m117() -> MethodBuilder {
    method("isFormatting")
        .private()
        .static_()
        .param("n", TypeExpr::Str)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_call("in_array", vec![e_var("n"), e_array(vec![e_str("a"), e_str("b"), e_str("big"), e_str("code"), e_str("em"), e_str("font"), e_str("i"), e_str("nobr"), e_str("s"), e_str("small"), e_str("strike"), e_str("strong"), e_str("tt"), e_str("u")]), e_bool(true)])),
        ])
}

/// `afeIndexOf` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m118() -> MethodBuilder {
    method("afeIndexOf")
        .private()
        .param("el", t_class("\\Dom\\Element"))
        .returns(TypeExpr::Int)
        .body(vec![
            s_for(Some(s_assign("i", e_binop(e_call("count", vec![e_this_prop("afe")]), BinOp::Sub, e_int(1)))), Some(e_binop(e_var("i"), BinOp::GtEq, e_int(0))), Some(s_expr(e_post_dec("i"))), vec![
                s_if(
                    e_binop(e_index(e_this_prop("afe"), e_var("i")), BinOp::StrictEq, e_var("el")),
                    vec![
                        s_return(e_var("i")),
                    ],
                    vec![],
                    None,
                ),
            ]),
            s_return(e_neg(e_int(1))),
        ])
}

/// `afePush` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m119() -> MethodBuilder {
    method("afePush")
        .private()
        .param("el", t_class("\\Dom\\Element"))
        .param("attrs", t_array())
        .keep_unread_params()
        .returns(TypeExpr::Void)
        .body(vec![
            s_assign("count", e_int(0)),
            s_for(Some(s_assign("i", e_binop(e_call("count", vec![e_this_prop("afe")]), BinOp::Sub, e_int(1)))), Some(e_binop(e_var("i"), BinOp::GtEq, e_int(0))), Some(s_expr(e_post_dec("i"))), vec![
                s_assign("entry", e_index(e_this_prop("afe"), e_var("i"))),
                s_if(
                    e_binop(e_var("entry"), BinOp::StrictEq, e_null()),
                    vec![
                        s_break(1),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_binop(e_prop(e_var("entry"), "localName"), BinOp::StrictEq, e_prop(e_var("el"), "localName")), BinOp::And, e_method_call(e_this(), "sameAttributes", vec![e_var("entry"), e_var("el")])),
                    vec![
                        s_expr(e_post_inc("count")),
                        s_if(
                            e_binop(e_var("count"), BinOp::StrictEq, e_int(3)),
                            vec![
                                s_expr(e_call("array_splice", vec![e_this_prop("afe"), e_var("i"), e_int(1)])),
                                s_break(1),
                            ],
                            vec![],
                            None,
                        ),
                    ],
                    vec![],
                    None,
                ),
            ]),
            s_prop_array_push(e_this(), "afe", e_var("el")),
        ])
}

/// `sameAttributes` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m120() -> MethodBuilder {
    method("sameAttributes")
        .private()
        .param("a", t_class("\\Dom\\Element"))
        .param("b", t_class("\\Dom\\Element"))
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_binop(e_prop(e_prop(e_var("a"), "attributes"), "length"), BinOp::StrictNotEq, e_prop(e_prop(e_var("b"), "attributes"), "length")),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_foreach(e_prop(e_prop(e_var("a"), "attributes"), "items"), None, "attr", vec![
                s_assign("other", e_method_call(e_var("b"), "getAttribute", vec![e_prop(e_var("attr"), "name")])),
                s_if(
                    e_binop(e_binop(e_var("other"), BinOp::StrictEq, e_null()), BinOp::Or, e_binop(e_var("other"), BinOp::StrictNotEq, e_prop(e_var("attr"), "value"))),
                    vec![
                        s_return(e_bool(false)),
                    ],
                    vec![],
                    None,
                ),
            ]),
            s_return(e_bool(true)),
        ])
}

/// `reconstructFormatting` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m121() -> MethodBuilder {
    method("reconstructFormatting")
        .private()
        .returns(TypeExpr::Void)
        .body(vec![
            s_assign("n", e_call("count", vec![e_this_prop("afe")])),
            s_if(
                e_binop(e_var("n"), BinOp::StrictEq, e_int(0)),
                vec![
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_assign("entry", e_index(e_this_prop("afe"), e_binop(e_var("n"), BinOp::Sub, e_int(1)))),
            s_if(
                e_binop(e_binop(e_var("entry"), BinOp::StrictEq, e_null()), BinOp::Or, e_binop(e_method_call(e_this(), "openIndex", vec![e_var("entry")]), BinOp::GtEq, e_int(0))),
                vec![
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_assign("i", e_binop(e_var("n"), BinOp::Sub, e_int(1))),
            s_while(e_binop(e_var("i"), BinOp::Gt, e_int(0)), vec![
                s_expr(e_post_dec("i")),
                s_assign("entry", e_index(e_this_prop("afe"), e_var("i"))),
                s_if(
                    e_binop(e_binop(e_var("entry"), BinOp::StrictEq, e_null()), BinOp::Or, e_binop(e_method_call(e_this(), "openIndex", vec![e_var("entry")]), BinOp::GtEq, e_int(0))),
                    vec![
                        s_expr(e_post_inc("i")),
                        s_break(1),
                    ],
                    vec![],
                    None,
                ),
            ]),
            s_for(None, Some(e_binop(e_var("i"), BinOp::Lt, e_var("n"))), Some(s_expr(e_post_inc("i"))), vec![
                s_assign("old", e_index(e_this_prop("afe"), e_var("i"))),
                s_assign("attrs", e_array(vec![])),
                s_foreach(e_prop(e_prop(e_var("old"), "attributes"), "items"), None, "attr", vec![
                    s_array_push("attrs", e_array(vec![e_prop(e_var("attr"), "name"), e_prop(e_var("attr"), "value")])),
                ]),
                s_assign("el", e_method_call(e_this(), "insertElement", vec![e_prop(e_var("old"), "localName"), e_var("attrs")])),
                s_expr(e_call("array_splice", vec![e_this_prop("afe"), e_var("i"), e_int(1), e_array(vec![e_var("el")])])),
            ]),
        ])
}

/// `clearFormattingToMarker` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m122() -> MethodBuilder {
    method("clearFormattingToMarker")
        .private()
        .returns(TypeExpr::Void)
        .body(vec![
            s_while(e_binop(e_call("count", vec![e_this_prop("afe")]), BinOp::Gt, e_int(0)), vec![
                s_assign("entry", e_call("array_pop", vec![e_this_prop("afe")])),
                s_if(
                    e_binop(e_var("entry"), BinOp::StrictEq, e_null()),
                    vec![
                        s_break(1),
                    ],
                    vec![],
                    None,
                ),
            ]),
        ])
}

/// `adoptionAgency` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m123() -> MethodBuilder {
    method("adoptionAgency")
        .private()
        .param("subject", TypeExpr::Str)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_assign("current", e_method_call(e_this(), "curr", vec![])),
            s_if(
                e_binop(e_binop(e_binop(e_var("current"), BinOp::StrictNotEq, e_null()), BinOp::And, e_binop(e_prop(e_var("current"), "localName"), BinOp::StrictEq, e_var("subject"))), BinOp::And, e_binop(e_method_call(e_this(), "afeIndexOf", vec![e_var("current")]), BinOp::Lt, e_int(0))),
                vec![
                    s_expr(e_method_call(e_this(), "popOpen", vec![])),
                    s_return(e_bool(true)),
                ],
                vec![],
                None,
            ),
            s_for(Some(s_assign("outer", e_int(0))), Some(e_binop(e_var("outer"), BinOp::Lt, e_int(8))), Some(s_expr(e_post_inc("outer"))), vec![
                s_assign("formatting", e_null()),
                s_for(Some(s_assign("i", e_binop(e_call("count", vec![e_this_prop("afe")]), BinOp::Sub, e_int(1)))), Some(e_binop(e_var("i"), BinOp::GtEq, e_int(0))), Some(s_expr(e_post_dec("i"))), vec![
                    s_assign("entry", e_index(e_this_prop("afe"), e_var("i"))),
                    s_if(
                        e_binop(e_var("entry"), BinOp::StrictEq, e_null()),
                        vec![
                            s_break(1),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_prop(e_var("entry"), "localName"), BinOp::StrictEq, e_var("subject")),
                        vec![
                            s_assign("formatting", e_var("entry")),
                            s_break(1),
                        ],
                        vec![],
                        None,
                    ),
                ]),
                s_if(
                    e_binop(e_var("formatting"), BinOp::StrictEq, e_null()),
                    vec![
                        s_return(e_bool(false)),
                    ],
                    vec![],
                    None,
                ),
                s_assign("fIndex", e_method_call(e_this(), "openIndex", vec![e_var("formatting")])),
                s_if(
                    e_binop(e_var("fIndex"), BinOp::Lt, e_int(0)),
                    vec![
                        s_expr(e_call("array_splice", vec![e_this_prop("afe"), e_method_call(e_this(), "afeIndexOf", vec![e_var("formatting")]), e_int(1)])),
                        s_return(e_bool(true)),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_not(e_method_call(e_this(), "inScope", vec![e_prop(e_var("formatting"), "localName")])),
                    vec![
                        s_return(e_bool(true)),
                    ],
                    vec![],
                    None,
                ),
                s_assign("furthest", e_null()),
                s_assign("furthestIndex", e_neg(e_int(1))),
                s_for(Some(s_assign("i", e_binop(e_var("fIndex"), BinOp::Add, e_int(1)))), Some(e_binop(e_var("i"), BinOp::Lt, e_call("count", vec![e_this_prop("open")]))), Some(s_expr(e_post_inc("i"))), vec![
                    s_if(
                        e_method_call(e_this(), "isSpecialEl", vec![e_index(e_this_prop("open"), e_var("i"))]),
                        vec![
                            s_assign("furthest", e_index(e_this_prop("open"), e_var("i"))),
                            s_assign("furthestIndex", e_var("i")),
                            s_break(1),
                        ],
                        vec![],
                        None,
                    ),
                ]),
                s_if(
                    e_binop(e_var("furthest"), BinOp::StrictEq, e_null()),
                    vec![
                        s_while(e_binop(e_call("count", vec![e_this_prop("open")]), BinOp::Gt, e_var("fIndex")), vec![
                            s_expr(e_call("array_pop", vec![e_this_prop("open")])),
                        ]),
                        s_expr(e_call("array_splice", vec![e_this_prop("afe"), e_method_call(e_this(), "afeIndexOf", vec![e_var("formatting")]), e_int(1)])),
                        s_return(e_bool(true)),
                    ],
                    vec![],
                    None,
                ),
                s_assign("commonAncestor", e_index(e_this_prop("open"), e_binop(e_var("fIndex"), BinOp::Sub, e_int(1)))),
                s_assign("bookmark", e_method_call(e_this(), "afeIndexOf", vec![e_var("formatting")])),
                s_assign("node", e_var("furthest")),
                s_assign("lastNode", e_var("furthest")),
                s_assign("nodeIndex", e_var("furthestIndex")),
                s_assign("inner", e_int(0)),
                s_while(e_bool(true), vec![
                    s_expr(e_post_inc("inner")),
                    s_expr(e_post_dec("nodeIndex")),
                    s_assign("node", e_index(e_this_prop("open"), e_var("nodeIndex"))),
                    s_if(
                        e_binop(e_var("node"), BinOp::StrictEq, e_var("formatting")),
                        vec![
                            s_break(1),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("afeNode", e_method_call(e_this(), "afeIndexOf", vec![e_var("node")])),
                    s_if(
                        e_binop(e_binop(e_var("inner"), BinOp::Gt, e_int(3)), BinOp::And, e_binop(e_var("afeNode"), BinOp::GtEq, e_int(0))),
                        vec![
                            s_expr(e_call("array_splice", vec![e_this_prop("afe"), e_var("afeNode"), e_int(1)])),
                            s_if(
                                e_binop(e_var("afeNode"), BinOp::Lt, e_var("bookmark")),
                                vec![
                                    s_expr(e_post_dec("bookmark")),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("afeNode", e_neg(e_int(1))),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("afeNode"), BinOp::Lt, e_int(0)),
                        vec![
                            s_expr(e_call("array_splice", vec![e_this_prop("open"), e_var("nodeIndex"), e_int(1)])),
                            s_continue(1),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("attrs", e_array(vec![])),
                    s_foreach(e_prop(e_prop(e_var("node"), "attributes"), "items"), None, "attr", vec![
                        s_array_push("attrs", e_array(vec![e_prop(e_var("attr"), "name"), e_prop(e_var("attr"), "value")])),
                    ]),
                    s_assign("replacement", e_method_call(e_this(), "makeElement", vec![e_prop(e_var("node"), "localName"), e_var("attrs")])),
                    s_expr(e_call("array_splice", vec![e_this_prop("afe"), e_var("afeNode"), e_int(1), e_array(vec![e_var("replacement")])])),
                    s_expr(e_call("array_splice", vec![e_this_prop("open"), e_var("nodeIndex"), e_int(1), e_array(vec![e_var("replacement")])])),
                    s_assign("node", e_var("replacement")),
                    s_if(
                        e_binop(e_var("lastNode"), BinOp::StrictEq, e_var("furthest")),
                        vec![
                            s_assign("bookmark", e_binop(e_var("afeNode"), BinOp::Add, e_int(1))),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_prop(e_var("lastNode"), "parentNode"), BinOp::StrictNotEq, e_null()),
                        vec![
                            s_expr(e_method_call(e_prop(e_var("lastNode"), "parentNode"), "removeChild", vec![e_var("lastNode")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_var("node"), "insertBefore", vec![e_var("lastNode"), e_null()])),
                    s_assign("lastNode", e_var("node")),
                ]),
                s_if(
                    e_binop(e_prop(e_var("lastNode"), "parentNode"), BinOp::StrictNotEq, e_null()),
                    vec![
                        s_expr(e_method_call(e_prop(e_var("lastNode"), "parentNode"), "removeChild", vec![e_var("lastNode")])),
                    ],
                    vec![],
                    None,
                ),
                s_expr(e_method_call(e_this(), "flushText", vec![])),
                s_assign("before", e_null()),
                s_assign("parent", e_method_call(e_this(), "insertionParent", vec![e_var("commonAncestor"), e_var("before")])),
                s_expr(e_method_call(e_var("parent"), "insertBefore", vec![e_var("lastNode"), e_var("before")])),
                s_assign("attrs", e_array(vec![])),
                s_foreach(e_prop(e_prop(e_var("formatting"), "attributes"), "items"), None, "attr", vec![
                    s_array_push("attrs", e_array(vec![e_prop(e_var("attr"), "name"), e_prop(e_var("attr"), "value")])),
                ]),
                s_assign("new", e_method_call(e_this(), "makeElement", vec![e_prop(e_var("formatting"), "localName"), e_var("attrs")])),
                s_while(e_binop(e_call("count", vec![e_prop(e_var("furthest"), "kids")]), BinOp::Gt, e_int(0)), vec![
                    s_assign("child", e_index(e_prop(e_var("furthest"), "kids"), e_int(0))),
                    s_expr(e_method_call(e_var("furthest"), "removeChild", vec![e_var("child")])),
                    s_expr(e_method_call(e_var("new"), "insertBefore", vec![e_var("child"), e_null()])),
                ]),
                s_expr(e_method_call(e_var("furthest"), "insertBefore", vec![e_var("new"), e_null()])),
                s_assign("old", e_method_call(e_this(), "afeIndexOf", vec![e_var("formatting")])),
                s_expr(e_call("array_splice", vec![e_this_prop("afe"), e_var("old"), e_int(1)])),
                s_if(
                    e_binop(e_var("old"), BinOp::Lt, e_var("bookmark")),
                    vec![
                        s_expr(e_post_dec("bookmark")),
                    ],
                    vec![],
                    None,
                ),
                s_expr(e_call("array_splice", vec![e_this_prop("afe"), e_var("bookmark"), e_int(0), e_array(vec![e_var("new")])])),
                s_expr(e_method_call(e_this(), "removeOpen", vec![e_var("formatting")])),
                s_assign("at", e_method_call(e_this(), "openIndex", vec![e_var("furthest")])),
                s_expr(e_call("array_splice", vec![e_this_prop("open"), e_binop(e_var("at"), BinOp::Add, e_int(1)), e_int(0), e_array(vec![e_var("new")])])),
            ]),
            s_return(e_bool(true)),
        ])
}

/// `resetMode` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m124() -> MethodBuilder {
    method("resetMode")
        .private()
        .returns(TypeExpr::Void)
        .body(vec![
            s_for(Some(s_assign("i", e_binop(e_call("count", vec![e_this_prop("open")]), BinOp::Sub, e_int(1)))), Some(e_binop(e_var("i"), BinOp::GtEq, e_int(0))), Some(s_expr(e_post_dec("i"))), vec![
                s_assign("n", e_prop(e_index(e_this_prop("open"), e_var("i")), "localName")),
                s_assign("last", e_binop(e_var("i"), BinOp::StrictEq, e_int(0))),
                s_if(
                    e_binop(e_var("n"), BinOp::StrictEq, e_str("select")),
                    vec![
                        s_prop_assign(e_this(), "mode", e_str("in select")),
                        s_return_void(),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_binop(e_binop(e_var("n"), BinOp::StrictEq, e_str("td")), BinOp::Or, e_binop(e_var("n"), BinOp::StrictEq, e_str("th"))), BinOp::And, e_not(e_var("last"))),
                    vec![
                        s_prop_assign(e_this(), "mode", e_str("in cell")),
                        s_return_void(),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_var("n"), BinOp::StrictEq, e_str("tr")),
                    vec![
                        s_prop_assign(e_this(), "mode", e_str("in row")),
                        s_return_void(),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_binop(e_binop(e_var("n"), BinOp::StrictEq, e_str("tbody")), BinOp::Or, e_binop(e_var("n"), BinOp::StrictEq, e_str("thead"))), BinOp::Or, e_binop(e_var("n"), BinOp::StrictEq, e_str("tfoot"))),
                    vec![
                        s_prop_assign(e_this(), "mode", e_str("in table body")),
                        s_return_void(),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_var("n"), BinOp::StrictEq, e_str("caption")),
                    vec![
                        s_prop_assign(e_this(), "mode", e_str("in caption")),
                        s_return_void(),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_var("n"), BinOp::StrictEq, e_str("colgroup")),
                    vec![
                        s_prop_assign(e_this(), "mode", e_str("in column group")),
                        s_return_void(),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_var("n"), BinOp::StrictEq, e_str("table")),
                    vec![
                        s_prop_assign(e_this(), "mode", e_str("in table")),
                        s_return_void(),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_var("n"), BinOp::StrictEq, e_str("template")),
                    vec![
                        s_prop_assign(e_this(), "mode", e_ternary(e_binop(e_call("count", vec![e_this_prop("templateModes")]), BinOp::Gt, e_int(0)), e_index(e_this_prop("templateModes"), e_binop(e_call("count", vec![e_this_prop("templateModes")]), BinOp::Sub, e_int(1))), e_str("in template"))),
                        s_return_void(),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_binop(e_var("n"), BinOp::StrictEq, e_str("head")), BinOp::And, e_not(e_var("last"))),
                    vec![
                        s_prop_assign(e_this(), "mode", e_str("in head")),
                        s_return_void(),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_var("n"), BinOp::StrictEq, e_str("body")),
                    vec![
                        s_prop_assign(e_this(), "mode", e_str("in body")),
                        s_return_void(),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_var("n"), BinOp::StrictEq, e_str("frameset")),
                    vec![
                        s_prop_assign(e_this(), "mode", e_str("in frameset")),
                        s_return_void(),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_var("n"), BinOp::StrictEq, e_str("html")),
                    vec![
                        s_prop_assign(e_this(), "mode", e_ternary(e_binop(e_this_prop("headEl"), BinOp::StrictEq, e_null()), e_str("before head"), e_str("after head"))),
                        s_return_void(),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_var("last"),
                    vec![
                        s_prop_assign(e_this(), "mode", e_str("in body")),
                        s_return_void(),
                    ],
                    vec![],
                    None,
                ),
            ]),
            s_prop_assign(e_this(), "mode", e_str("in body")),
        ])
}

/// `tbDoctype` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m125() -> MethodBuilder {
    method("tbDoctype")
        .private()
        .param("name", TypeExpr::Str)
        .param("public", t_nullable(TypeExpr::Str))
        .param("system", t_nullable(TypeExpr::Str))
        .param("forceQuirks", TypeExpr::Bool)
        .returns(TypeExpr::Void)
        .body(vec![
            s_if(
                e_binop(e_this_prop("mode"), BinOp::StrictNotEq, e_str("initial")),
                vec![
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_assign("doctype", e_new_fq("Dom\\DocumentType", vec![e_var("name"), e_null_coalesce(e_var("public"), e_str("")), e_null_coalesce(e_var("system"), e_str(""))])),
            s_prop_assign(e_var("doctype"), "ownerDocument", e_this_prop("doc")),
            s_expr(e_method_call(e_this_prop("doc"), "insertBefore", vec![e_var("doctype"), e_null()])),
            s_if(
                e_binop(e_binop(e_binop(e_binop(e_var("name"), BinOp::StrictEq, e_str("html")), BinOp::And, e_not(e_var("forceQuirks"))), BinOp::And, e_binop(e_var("public"), BinOp::StrictEq, e_null())), BinOp::And, e_binop(e_var("system"), BinOp::StrictEq, e_null())),
                vec![
                    s_prop_assign(e_this(), "quirks", e_bool(false)),
                ],
                vec![
                (e_binop(e_binop(e_var("name"), BinOp::StrictEq, e_str("html")), BinOp::And, e_not(e_var("forceQuirks"))), vec![
                    s_prop_assign(e_this(), "quirks", e_bool(false)),
                ]),
            ],
                Some(vec![
                s_prop_assign(e_this(), "quirks", e_bool(true)),
            ]),
            ),
            s_prop_assign(e_this(), "mode", e_str("before html")),
        ])
}

/// `tbComment` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m126() -> MethodBuilder {
    method("tbComment")
        .private()
        .param("text", TypeExpr::Str)
        .returns(TypeExpr::Void)
        .body(vec![
            s_assign("comment", e_new_fq("Dom\\Comment", vec![e_var("text")])),
            s_prop_assign(e_var("comment"), "ownerDocument", e_this_prop("doc")),
            s_if(
                e_binop(e_binop(e_binop(e_this_prop("mode"), BinOp::StrictEq, e_str("initial")), BinOp::Or, e_binop(e_this_prop("mode"), BinOp::StrictEq, e_str("before html"))), BinOp::Or, e_binop(e_this_prop("mode"), BinOp::StrictEq, e_str("after after body"))),
                vec![
                    s_expr(e_method_call(e_this_prop("doc"), "insertBefore", vec![e_var("comment"), e_null()])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_this_prop("mode"), BinOp::StrictEq, e_str("after body")),
                vec![
                    s_assign("html", e_index(e_this_prop("open"), e_int(0))),
                    s_expr(e_method_call(e_var("html"), "insertBefore", vec![e_var("comment"), e_null()])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_expr(e_method_call(e_this(), "insertNode", vec![e_var("comment")])),
        ])
}

/// `tbChars` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m127() -> MethodBuilder {
    method("tbChars")
        .private()
        .param("text", TypeExpr::Str)
        .returns(TypeExpr::Void)
        .body(vec![
            s_assign("remaining", e_var("text")),
            s_if(
                e_binop(e_binop(e_binop(e_binop(e_binop(e_this_prop("mode"), BinOp::StrictEq, e_str("in table")), BinOp::Or, e_binop(e_this_prop("mode"), BinOp::StrictEq, e_str("in table body"))), BinOp::Or, e_binop(e_this_prop("mode"), BinOp::StrictEq, e_str("in row"))), BinOp::And, e_call("in_array", vec![e_method_call(e_this(), "currName", vec![]), e_array(vec![e_str("table"), e_str("tbody"), e_str("tfoot"), e_str("thead"), e_str("tr")]), e_bool(true)])), BinOp::And, e_not(e_method_call(e_this(), "inForeign", vec![e_str("chars"), e_str("")]))),
                vec![
                    s_prop_assign(e_this(), "tableTextBuf", e_binop(e_this_prop("tableTextBuf"), BinOp::Concat, e_var("text"))),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_method_call(e_this(), "inForeign", vec![e_str("chars"), e_str("")]),
                vec![
                    s_expr(e_method_call(e_this(), "insertCharsNow", vec![e_call("str_replace", vec![e_str("\0"), e_str("�"), e_var("remaining")])])),
                    s_if(
                        e_binop(e_call("trim", vec![e_var("remaining"), e_str(" \t\n\u{c}")]), BinOp::StrictNotEq, e_str("")),
                        vec![
                            s_prop_assign(e_this(), "framesetOk", e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_while(e_binop(e_var("remaining"), BinOp::StrictNotEq, e_str("")), vec![
                s_assign("n", e_call("strlen", vec![e_var("remaining")])),
                s_assign("i", e_int(0)),
                s_while(e_binop(e_binop(e_var("i"), BinOp::Lt, e_var("n")), BinOp::And, e_self_call("isSpace", vec![e_index(e_var("remaining"), e_var("i"))])), vec![
                    s_expr(e_post_inc("i")),
                ]),
                s_if(
                    e_binop(e_var("i"), BinOp::Gt, e_int(0)),
                    vec![
                        s_expr(e_method_call(e_this(), "tbWhitespace", vec![e_call("substr", vec![e_var("remaining"), e_int(0), e_var("i")])])),
                        s_assign("remaining", e_call("substr", vec![e_var("remaining"), e_var("i")])),
                        s_continue(1),
                    ],
                    vec![],
                    None,
                ),
                s_assign("j", e_int(0)),
                s_while(e_binop(e_binop(e_var("j"), BinOp::Lt, e_var("n")), BinOp::And, e_not(e_self_call("isSpace", vec![e_index(e_var("remaining"), e_var("j"))]))), vec![
                    s_expr(e_post_inc("j")),
                ]),
                s_expr(e_method_call(e_this(), "tbText", vec![e_call("substr", vec![e_var("remaining"), e_int(0), e_var("j")])])),
                s_assign("remaining", e_call("substr", vec![e_var("remaining"), e_var("j")])),
            ]),
        ])
}

/// `tbWhitespace` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m128() -> MethodBuilder {
    method("tbWhitespace")
        .private()
        .param("ws", TypeExpr::Str)
        .returns(TypeExpr::Void)
        .body(vec![
            s_assign("m", e_this_prop("mode")),
            s_if(
                e_binop(e_binop(e_binop(e_var("m"), BinOp::StrictEq, e_str("initial")), BinOp::Or, e_binop(e_var("m"), BinOp::StrictEq, e_str("before html"))), BinOp::Or, e_binop(e_var("m"), BinOp::StrictEq, e_str("before head"))),
                vec![
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_var("m"), BinOp::StrictEq, e_str("in head")), BinOp::Or, e_binop(e_var("m"), BinOp::StrictEq, e_str("in head noscript"))), BinOp::Or, e_binop(e_var("m"), BinOp::StrictEq, e_str("in column group"))), BinOp::Or, e_binop(e_var("m"), BinOp::StrictEq, e_str("after head"))), BinOp::Or, e_binop(e_var("m"), BinOp::StrictEq, e_str("in frameset"))), BinOp::Or, e_binop(e_var("m"), BinOp::StrictEq, e_str("after frameset"))),
                vec![
                    s_if(
                        e_binop(e_var("m"), BinOp::StrictEq, e_str("in head noscript")),
                        vec![
                            s_expr(e_method_call(e_this(), "insertCharsNow", vec![e_var("ws")])),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "insertCharsNow", vec![e_var("ws")])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_binop(e_var("m"), BinOp::StrictEq, e_str("after body")), BinOp::Or, e_binop(e_var("m"), BinOp::StrictEq, e_str("after after body"))), BinOp::Or, e_binop(e_var("m"), BinOp::StrictEq, e_str("after after frameset"))),
                vec![
                    s_expr(e_method_call(e_this(), "processUsing", vec![e_str("in body"), closure()
                        .captures("ws")
                        .body(vec![
                            s_expr(e_method_call(e_this(), "reconstructFormatting", vec![])),
                            s_expr(e_method_call(e_this(), "insertCharsNow", vec![e_var("ws")])),
                        ])
                        .build()])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_binop(e_var("m"), BinOp::StrictEq, e_str("in table")), BinOp::Or, e_binop(e_var("m"), BinOp::StrictEq, e_str("in table body"))), BinOp::Or, e_binop(e_var("m"), BinOp::StrictEq, e_str("in row"))),
                vec![
                    s_assign("c", e_method_call(e_this(), "currName", vec![])),
                    s_if(
                        e_call("in_array", vec![e_var("c"), e_array(vec![e_str("table"), e_str("tbody"), e_str("tfoot"), e_str("thead"), e_str("tr")]), e_bool(true)]),
                        vec![
                            s_expr(e_method_call(e_this(), "insertCharsNow", vec![e_var("ws")])),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "bodyChars", vec![e_var("ws")])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("m"), BinOp::StrictEq, e_str("in select")),
                vec![
                    s_expr(e_method_call(e_this(), "insertCharsNow", vec![e_var("ws")])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("m"), BinOp::StrictEq, e_str("text")),
                vec![
                    s_expr(e_method_call(e_this(), "insertCharsNow", vec![e_var("ws")])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_expr(e_method_call(e_this(), "bodyChars", vec![e_var("ws")])),
        ])
}

/// `bodyChars` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m129() -> MethodBuilder {
    method("bodyChars")
        .private()
        .param("text", TypeExpr::Str)
        .returns(TypeExpr::Void)
        .body(vec![
            s_expr(e_method_call(e_this(), "reconstructFormatting", vec![])),
            s_expr(e_method_call(e_this(), "insertCharsNow", vec![e_var("text")])),
            s_if(
                e_binop(e_call("trim", vec![e_var("text"), e_str(" \t\n\u{c}")]), BinOp::StrictNotEq, e_str("")),
                vec![
                    s_prop_assign(e_this(), "framesetOk", e_bool(false)),
                ],
                vec![],
                None,
            ),
        ])
}

/// `processUsing` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m130() -> MethodBuilder {
    method("processUsing")
        .private()
        .param("mode", TypeExpr::Str)
        .param("fn", t_class("\\Closure"))
        .keep_unread_params()
        .returns(TypeExpr::Void)
        .body(vec![
            s_assign("saved", e_this_prop("mode")),
            s_prop_assign(e_this(), "mode", e_var("mode")),
            s_expr(e_closure_call("fn", vec![])),
            s_if(
                e_binop(e_this_prop("mode"), BinOp::StrictEq, e_var("mode")),
                vec![
                    s_prop_assign(e_this(), "mode", e_var("saved")),
                ],
                vec![],
                None,
            ),
        ])
}

/// `tbText` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m131() -> MethodBuilder {
    method("tbText")
        .private()
        .param("text", TypeExpr::Str)
        .returns(TypeExpr::Void)
        .body(vec![
            s_expr(e_method_call(e_this(), "flushText", vec![])),
            s_assign("m", e_this_prop("mode")),
            s_if(
                e_binop(e_var("m"), BinOp::StrictEq, e_str("initial")),
                vec![
                    s_prop_assign(e_this(), "quirks", e_bool(true)),
                    s_prop_assign(e_this(), "mode", e_str("before html")),
                    s_expr(e_method_call(e_this(), "tbText", vec![e_var("text")])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("m"), BinOp::StrictEq, e_str("before html")),
                vec![
                    s_expr(e_method_call(e_this(), "createHtml", vec![e_array(vec![])])),
                    s_prop_assign(e_this(), "mode", e_str("before head")),
                    s_expr(e_method_call(e_this(), "tbText", vec![e_var("text")])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("m"), BinOp::StrictEq, e_str("before head")),
                vec![
                    s_prop_assign(e_this(), "headEl", e_method_call(e_this(), "insertElement", vec![e_str("head"), e_array(vec![])])),
                    s_prop_assign(e_this(), "mode", e_str("in head")),
                    s_expr(e_method_call(e_this(), "tbText", vec![e_var("text")])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("m"), BinOp::StrictEq, e_str("in head")),
                vec![
                    s_expr(e_method_call(e_this(), "popOpen", vec![])),
                    s_prop_assign(e_this(), "mode", e_str("after head")),
                    s_expr(e_method_call(e_this(), "tbText", vec![e_var("text")])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("m"), BinOp::StrictEq, e_str("in head noscript")),
                vec![
                    s_expr(e_method_call(e_this(), "popOpen", vec![])),
                    s_prop_assign(e_this(), "mode", e_str("in head")),
                    s_expr(e_method_call(e_this(), "tbText", vec![e_var("text")])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("m"), BinOp::StrictEq, e_str("after head")),
                vec![
                    s_expr(e_method_call(e_this(), "insertElement", vec![e_str("body"), e_array(vec![])])),
                    s_prop_assign(e_this(), "mode", e_str("in body")),
                    s_expr(e_method_call(e_this(), "tbText", vec![e_var("text")])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("m"), BinOp::StrictEq, e_str("text")),
                vec![
                    s_expr(e_method_call(e_this(), "insertCharsNow", vec![e_var("text")])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_binop(e_var("m"), BinOp::StrictEq, e_str("in table")), BinOp::Or, e_binop(e_var("m"), BinOp::StrictEq, e_str("in table body"))), BinOp::Or, e_binop(e_var("m"), BinOp::StrictEq, e_str("in row"))),
                vec![
                    s_assign("c", e_method_call(e_this(), "currName", vec![])),
                    s_if(
                        e_call("in_array", vec![e_var("c"), e_array(vec![e_str("table"), e_str("tbody"), e_str("tfoot"), e_str("thead"), e_str("tr")]), e_bool(true)]),
                        vec![
                            s_assign("saved", e_this_prop("foster")),
                            s_prop_assign(e_this(), "foster", e_bool(true)),
                            s_expr(e_method_call(e_this(), "flushText", vec![])),
                            s_expr(e_method_call(e_this(), "bodyChars", vec![e_var("text")])),
                            s_expr(e_method_call(e_this(), "flushText", vec![])),
                            s_prop_assign(e_this(), "foster", e_var("saved")),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "bodyChars", vec![e_var("text")])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("m"), BinOp::StrictEq, e_str("in column group")),
                vec![
                    s_if(
                        e_binop(e_method_call(e_this(), "currName", vec![]), BinOp::StrictEq, e_str("colgroup")),
                        vec![
                            s_expr(e_method_call(e_this(), "popOpen", vec![])),
                            s_prop_assign(e_this(), "mode", e_str("in table")),
                            s_expr(e_method_call(e_this(), "tbText", vec![e_var("text")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("m"), BinOp::StrictEq, e_str("in select")),
                vec![
                    s_expr(e_method_call(e_this(), "insertCharsNow", vec![e_var("text")])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_var("m"), BinOp::StrictEq, e_str("after body")), BinOp::Or, e_binop(e_var("m"), BinOp::StrictEq, e_str("after after body"))),
                vec![
                    s_prop_assign(e_this(), "mode", e_str("in body")),
                    s_expr(e_method_call(e_this(), "tbText", vec![e_var("text")])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_binop(e_var("m"), BinOp::StrictEq, e_str("in frameset")), BinOp::Or, e_binop(e_var("m"), BinOp::StrictEq, e_str("after frameset"))), BinOp::Or, e_binop(e_var("m"), BinOp::StrictEq, e_str("after after frameset"))),
                vec![
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_expr(e_method_call(e_this(), "bodyChars", vec![e_var("text")])),
        ])
}

/// `createHtml` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m132() -> MethodBuilder {
    method("createHtml")
        .private()
        .param("attrs", t_array())
        .returns(t_class("\\Dom\\Element"))
        .body(vec![
            s_assign("el", e_method_call(e_this(), "makeElement", vec![e_str("html"), e_var("attrs")])),
            s_expr(e_method_call(e_this_prop("doc"), "insertBefore", vec![e_var("el"), e_null()])),
            s_expr(e_method_call(e_this(), "pushOpen", vec![e_var("el")])),
            s_return(e_var("el")),
        ])
}

/// `mergeAttributes` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m133() -> MethodBuilder {
    method("mergeAttributes")
        .private()
        .param("into", t_class("\\Dom\\Element"))
        .param("attrs", t_array())
        .returns(TypeExpr::Void)
        .body(vec![
            s_foreach(e_var("attrs"), None, "pair", vec![
                s_if(
                    e_not(e_method_call(e_var("into"), "hasAttribute", vec![e_index(e_var("pair"), e_int(0))])),
                    vec![
                        s_expr(e_method_call(e_var("into"), "setAttribute", vec![e_index(e_var("pair"), e_int(0)), e_index(e_var("pair"), e_int(1))])),
                    ],
                    vec![],
                    None,
                ),
            ]),
        ])
}

/// `tbStart` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m134() -> MethodBuilder {
    method("tbStart")
        .private()
        .param("name", TypeExpr::Str)
        .param("attrs", t_array())
        .param("selfClosing", TypeExpr::Bool)
        .returns(TypeExpr::Void)
        .body(vec![
            s_expr(e_method_call(e_this(), "flushText", vec![])),
            s_if(
                e_method_call(e_this(), "inForeign", vec![e_str("start"), e_var("name")]),
                vec![
                    s_expr(e_method_call(e_this(), "foreignStart", vec![e_var("name"), e_var("attrs"), e_var("selfClosing")])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_expr(e_method_call(e_this(), "dispatchStart", vec![e_var("name"), e_var("attrs"), e_var("selfClosing")])),
        ])
}

/// `dispatchStart` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m135() -> MethodBuilder {
    method("dispatchStart")
        .private()
        .param("name", TypeExpr::Str)
        .param("attrs", t_array())
        .param("selfClosing", TypeExpr::Bool)
        .returns(TypeExpr::Void)
        .body(vec![
            s_assign("m", e_this_prop("mode")),
            s_if(
                e_binop(e_var("m"), BinOp::StrictEq, e_str("initial")),
                vec![
                    s_prop_assign(e_this(), "quirks", e_bool(true)),
                    s_prop_assign(e_this(), "mode", e_str("before html")),
                    s_expr(e_method_call(e_this(), "dispatchStart", vec![e_var("name"), e_var("attrs"), e_var("selfClosing")])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("m"), BinOp::StrictEq, e_str("before html")),
                vec![
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("html")),
                        vec![
                            s_expr(e_method_call(e_this(), "createHtml", vec![e_var("attrs")])),
                            s_prop_assign(e_this(), "mode", e_str("before head")),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "createHtml", vec![e_array(vec![])])),
                    s_prop_assign(e_this(), "mode", e_str("before head")),
                    s_expr(e_method_call(e_this(), "dispatchStart", vec![e_var("name"), e_var("attrs"), e_var("selfClosing")])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("m"), BinOp::StrictEq, e_str("before head")),
                vec![
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("html")),
                        vec![
                            s_expr(e_method_call(e_this(), "startInBody", vec![e_var("name"), e_var("attrs"), e_var("selfClosing")])),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("head")),
                        vec![
                            s_prop_assign(e_this(), "headEl", e_method_call(e_this(), "insertElement", vec![e_str("head"), e_var("attrs")])),
                            s_prop_assign(e_this(), "mode", e_str("in head")),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "headEl", e_method_call(e_this(), "insertElement", vec![e_str("head"), e_array(vec![])])),
                    s_prop_assign(e_this(), "mode", e_str("in head")),
                    s_expr(e_method_call(e_this(), "dispatchStart", vec![e_var("name"), e_var("attrs"), e_var("selfClosing")])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("m"), BinOp::StrictEq, e_str("in head")),
                vec![
                    s_expr(e_method_call(e_this(), "startInHead", vec![e_var("name"), e_var("attrs"), e_var("selfClosing")])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("m"), BinOp::StrictEq, e_str("in head noscript")),
                vec![
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("html")),
                        vec![
                            s_expr(e_method_call(e_this(), "startInBody", vec![e_var("name"), e_var("attrs"), e_var("selfClosing")])),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_call("in_array", vec![e_var("name"), e_array(vec![e_str("basefont"), e_str("bgsound"), e_str("link"), e_str("meta"), e_str("noframes"), e_str("style")]), e_bool(true)]),
                        vec![
                            s_expr(e_method_call(e_this(), "startInHead", vec![e_var("name"), e_var("attrs"), e_var("selfClosing")])),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_var("name"), BinOp::StrictEq, e_str("head")), BinOp::Or, e_binop(e_var("name"), BinOp::StrictEq, e_str("noscript"))),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "popOpen", vec![])),
                    s_prop_assign(e_this(), "mode", e_str("in head")),
                    s_expr(e_method_call(e_this(), "dispatchStart", vec![e_var("name"), e_var("attrs"), e_var("selfClosing")])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("m"), BinOp::StrictEq, e_str("after head")),
                vec![
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("html")),
                        vec![
                            s_expr(e_method_call(e_this(), "startInBody", vec![e_var("name"), e_var("attrs"), e_var("selfClosing")])),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("body")),
                        vec![
                            s_expr(e_method_call(e_this(), "insertElement", vec![e_str("body"), e_var("attrs")])),
                            s_prop_assign(e_this(), "framesetOk", e_bool(false)),
                            s_prop_assign(e_this(), "mode", e_str("in body")),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("frameset")),
                        vec![
                            s_expr(e_method_call(e_this(), "insertElement", vec![e_str("frameset"), e_var("attrs")])),
                            s_prop_assign(e_this(), "mode", e_str("in frameset")),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_call("in_array", vec![e_var("name"), e_array(vec![e_str("base"), e_str("basefont"), e_str("bgsound"), e_str("link"), e_str("meta"), e_str("noframes"), e_str("script"), e_str("style"), e_str("template"), e_str("title")]), e_bool(true)]),
                        vec![
                            s_assign("head", e_this_prop("headEl")),
                            s_expr(e_method_call(e_this(), "pushOpen", vec![e_var("head")])),
                            s_expr(e_method_call(e_this(), "startInHead", vec![e_var("name"), e_var("attrs"), e_var("selfClosing")])),
                            s_expr(e_method_call(e_this(), "removeOpen", vec![e_var("head")])),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("head")),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "insertElement", vec![e_str("body"), e_array(vec![])])),
                    s_prop_assign(e_this(), "mode", e_str("in body")),
                    s_expr(e_method_call(e_this(), "dispatchStart", vec![e_var("name"), e_var("attrs"), e_var("selfClosing")])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("m"), BinOp::StrictEq, e_str("text")),
                vec![
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("m"), BinOp::StrictEq, e_str("in table")),
                vec![
                    s_expr(e_method_call(e_this(), "startInTable", vec![e_var("name"), e_var("attrs"), e_var("selfClosing")])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("m"), BinOp::StrictEq, e_str("in caption")),
                vec![
                    s_if(
                        e_call("in_array", vec![e_var("name"), e_array(vec![e_str("caption"), e_str("col"), e_str("colgroup"), e_str("tbody"), e_str("td"), e_str("tfoot"), e_str("th"), e_str("thead"), e_str("tr")]), e_bool(true)]),
                        vec![
                            s_if(
                                e_method_call(e_this(), "inScope", vec![e_str("caption"), e_str("table")]),
                                vec![
                                    s_expr(e_method_call(e_this(), "generateImpliedEndTags", vec![])),
                                    s_expr(e_method_call(e_this(), "popUntil", vec![e_str("caption")])),
                                    s_expr(e_method_call(e_this(), "clearFormattingToMarker", vec![])),
                                    s_prop_assign(e_this(), "mode", e_str("in table")),
                                    s_expr(e_method_call(e_this(), "dispatchStart", vec![e_var("name"), e_var("attrs"), e_var("selfClosing")])),
                                ],
                                vec![],
                                None,
                            ),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "startInBody", vec![e_var("name"), e_var("attrs"), e_var("selfClosing")])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("m"), BinOp::StrictEq, e_str("in column group")),
                vec![
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("html")),
                        vec![
                            s_expr(e_method_call(e_this(), "startInBody", vec![e_var("name"), e_var("attrs"), e_var("selfClosing")])),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("col")),
                        vec![
                            s_expr(e_method_call(e_this(), "insertVoid", vec![e_str("col"), e_var("attrs")])),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("template")),
                        vec![
                            s_expr(e_method_call(e_this(), "startInHead", vec![e_var("name"), e_var("attrs"), e_var("selfClosing")])),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_method_call(e_this(), "currName", vec![]), BinOp::StrictNotEq, e_str("colgroup")),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "popOpen", vec![])),
                    s_prop_assign(e_this(), "mode", e_str("in table")),
                    s_expr(e_method_call(e_this(), "dispatchStart", vec![e_var("name"), e_var("attrs"), e_var("selfClosing")])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("m"), BinOp::StrictEq, e_str("in table body")),
                vec![
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("tr")),
                        vec![
                            s_expr(e_method_call(e_this(), "clearToContext", vec![e_array(vec![e_str("tbody"), e_str("tfoot"), e_str("thead"), e_str("template"), e_str("html")])])),
                            s_expr(e_method_call(e_this(), "insertElement", vec![e_str("tr"), e_var("attrs")])),
                            s_prop_assign(e_this(), "mode", e_str("in row")),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_var("name"), BinOp::StrictEq, e_str("th")), BinOp::Or, e_binop(e_var("name"), BinOp::StrictEq, e_str("td"))),
                        vec![
                            s_expr(e_method_call(e_this(), "clearToContext", vec![e_array(vec![e_str("tbody"), e_str("tfoot"), e_str("thead"), e_str("template"), e_str("html")])])),
                            s_expr(e_method_call(e_this(), "insertElement", vec![e_str("tr"), e_array(vec![])])),
                            s_prop_assign(e_this(), "mode", e_str("in row")),
                            s_expr(e_method_call(e_this(), "dispatchStart", vec![e_var("name"), e_var("attrs"), e_var("selfClosing")])),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_call("in_array", vec![e_var("name"), e_array(vec![e_str("caption"), e_str("col"), e_str("colgroup"), e_str("tbody"), e_str("tfoot"), e_str("thead")]), e_bool(true)]),
                        vec![
                            s_if(
                                e_not(e_method_call(e_this(), "tableBodyInScope", vec![])),
                                vec![
                                    s_return_void(),
                                ],
                                vec![],
                                None,
                            ),
                            s_expr(e_method_call(e_this(), "clearToContext", vec![e_array(vec![e_str("tbody"), e_str("tfoot"), e_str("thead"), e_str("template"), e_str("html")])])),
                            s_expr(e_method_call(e_this(), "popOpen", vec![])),
                            s_prop_assign(e_this(), "mode", e_str("in table")),
                            s_expr(e_method_call(e_this(), "dispatchStart", vec![e_var("name"), e_var("attrs"), e_var("selfClosing")])),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "startInTable", vec![e_var("name"), e_var("attrs"), e_var("selfClosing")])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("m"), BinOp::StrictEq, e_str("in row")),
                vec![
                    s_if(
                        e_binop(e_binop(e_var("name"), BinOp::StrictEq, e_str("th")), BinOp::Or, e_binop(e_var("name"), BinOp::StrictEq, e_str("td"))),
                        vec![
                            s_expr(e_method_call(e_this(), "clearToContext", vec![e_array(vec![e_str("tr"), e_str("template"), e_str("html")])])),
                            s_expr(e_method_call(e_this(), "insertElement", vec![e_var("name"), e_var("attrs")])),
                            s_prop_assign(e_this(), "mode", e_str("in cell")),
                            s_prop_array_push(e_this(), "afe", e_null()),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_call("in_array", vec![e_var("name"), e_array(vec![e_str("caption"), e_str("col"), e_str("colgroup"), e_str("tbody"), e_str("tfoot"), e_str("thead"), e_str("tr")]), e_bool(true)]),
                        vec![
                            s_if(
                                e_not(e_method_call(e_this(), "inScope", vec![e_str("tr"), e_str("table")])),
                                vec![
                                    s_return_void(),
                                ],
                                vec![],
                                None,
                            ),
                            s_expr(e_method_call(e_this(), "clearToContext", vec![e_array(vec![e_str("tr"), e_str("template"), e_str("html")])])),
                            s_expr(e_method_call(e_this(), "popOpen", vec![])),
                            s_prop_assign(e_this(), "mode", e_str("in table body")),
                            s_expr(e_method_call(e_this(), "dispatchStart", vec![e_var("name"), e_var("attrs"), e_var("selfClosing")])),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "startInTable", vec![e_var("name"), e_var("attrs"), e_var("selfClosing")])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("m"), BinOp::StrictEq, e_str("in cell")),
                vec![
                    s_if(
                        e_call("in_array", vec![e_var("name"), e_array(vec![e_str("caption"), e_str("col"), e_str("colgroup"), e_str("tbody"), e_str("td"), e_str("tfoot"), e_str("th"), e_str("thead"), e_str("tr")]), e_bool(true)]),
                        vec![
                            s_if(
                                e_binop(e_not(e_method_call(e_this(), "inScope", vec![e_str("td"), e_str("table")])), BinOp::And, e_not(e_method_call(e_this(), "inScope", vec![e_str("th"), e_str("table")]))),
                                vec![
                                    s_return_void(),
                                ],
                                vec![],
                                None,
                            ),
                            s_expr(e_method_call(e_this(), "closeCell", vec![])),
                            s_expr(e_method_call(e_this(), "dispatchStart", vec![e_var("name"), e_var("attrs"), e_var("selfClosing")])),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "startInBody", vec![e_var("name"), e_var("attrs"), e_var("selfClosing")])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("m"), BinOp::StrictEq, e_str("in select")),
                vec![
                    s_expr(e_method_call(e_this(), "startInSelect", vec![e_var("name"), e_var("attrs"), e_var("selfClosing")])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("m"), BinOp::StrictEq, e_str("after body")),
                vec![
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("html")),
                        vec![
                            s_expr(e_method_call(e_this(), "startInBody", vec![e_var("name"), e_var("attrs"), e_var("selfClosing")])),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "mode", e_str("in body")),
                    s_expr(e_method_call(e_this(), "dispatchStart", vec![e_var("name"), e_var("attrs"), e_var("selfClosing")])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("m"), BinOp::StrictEq, e_str("after after body")),
                vec![
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("html")),
                        vec![
                            s_expr(e_method_call(e_this(), "startInBody", vec![e_var("name"), e_var("attrs"), e_var("selfClosing")])),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "mode", e_str("in body")),
                    s_expr(e_method_call(e_this(), "dispatchStart", vec![e_var("name"), e_var("attrs"), e_var("selfClosing")])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("m"), BinOp::StrictEq, e_str("in frameset")),
                vec![
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("html")),
                        vec![
                            s_expr(e_method_call(e_this(), "startInBody", vec![e_var("name"), e_var("attrs"), e_var("selfClosing")])),
                        ],
                        vec![
                        (e_binop(e_var("name"), BinOp::StrictEq, e_str("frameset")), vec![
                            s_expr(e_method_call(e_this(), "insertElement", vec![e_str("frameset"), e_var("attrs")])),
                        ]),
                        (e_binop(e_var("name"), BinOp::StrictEq, e_str("frame")), vec![
                            s_expr(e_method_call(e_this(), "insertVoid", vec![e_str("frame"), e_var("attrs")])),
                        ]),
                        (e_binop(e_var("name"), BinOp::StrictEq, e_str("noframes")), vec![
                            s_expr(e_method_call(e_this(), "startInHead", vec![e_var("name"), e_var("attrs"), e_var("selfClosing")])),
                        ]),
                    ],
                        None,
                    ),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_var("m"), BinOp::StrictEq, e_str("after frameset")), BinOp::Or, e_binop(e_var("m"), BinOp::StrictEq, e_str("after after frameset"))),
                vec![
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("html")),
                        vec![
                            s_expr(e_method_call(e_this(), "startInBody", vec![e_var("name"), e_var("attrs"), e_var("selfClosing")])),
                        ],
                        vec![
                        (e_binop(e_var("name"), BinOp::StrictEq, e_str("noframes")), vec![
                            s_expr(e_method_call(e_this(), "startInHead", vec![e_var("name"), e_var("attrs"), e_var("selfClosing")])),
                        ]),
                    ],
                        None,
                    ),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("m"), BinOp::StrictEq, e_str("in template")),
                vec![
                    s_expr(e_method_call(e_this(), "startInBody", vec![e_var("name"), e_var("attrs"), e_var("selfClosing")])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_expr(e_method_call(e_this(), "startInBody", vec![e_var("name"), e_var("attrs"), e_var("selfClosing")])),
        ])
}

/// `clearToContext` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m136() -> MethodBuilder {
    method("clearToContext")
        .private()
        .param("names", t_array())
        .returns(TypeExpr::Void)
        .body(vec![
            s_while(e_binop(e_binop(e_call("count", vec![e_this_prop("open")]), BinOp::Gt, e_int(0)), BinOp::And, e_not(e_call("in_array", vec![e_method_call(e_this(), "currName", vec![]), e_var("names"), e_bool(true)]))), vec![
                s_expr(e_method_call(e_this(), "popOpen", vec![])),
            ]),
        ])
}

/// `tableBodyInScope` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m137() -> MethodBuilder {
    method("tableBodyInScope")
        .private()
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_binop(e_binop(e_method_call(e_this(), "inScope", vec![e_str("tbody"), e_str("table")]), BinOp::Or, e_method_call(e_this(), "inScope", vec![e_str("thead"), e_str("table")])), BinOp::Or, e_method_call(e_this(), "inScope", vec![e_str("tfoot"), e_str("table")]))),
        ])
}

/// `closeCell` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m138() -> MethodBuilder {
    method("closeCell")
        .private()
        .returns(TypeExpr::Void)
        .body(vec![
            s_expr(e_method_call(e_this(), "generateImpliedEndTags", vec![])),
            s_expr(e_method_call(e_this(), "popUntilOneOf", vec![e_array(vec![e_str("td"), e_str("th")])])),
            s_expr(e_method_call(e_this(), "clearFormattingToMarker", vec![])),
            s_prop_assign(e_this(), "mode", e_str("in row")),
        ])
}

/// `startInHead` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m139() -> MethodBuilder {
    method("startInHead")
        .private()
        .param("name", TypeExpr::Str)
        .param("attrs", t_array())
        .param("selfClosing", TypeExpr::Bool)
        .returns(TypeExpr::Void)
        .body(vec![
            s_if(
                e_binop(e_var("name"), BinOp::StrictEq, e_str("html")),
                vec![
                    s_expr(e_method_call(e_this(), "startInBody", vec![e_var("name"), e_var("attrs"), e_var("selfClosing")])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_call("in_array", vec![e_var("name"), e_array(vec![e_str("base"), e_str("basefont"), e_str("bgsound"), e_str("link"), e_str("meta")]), e_bool(true)]),
                vec![
                    s_expr(e_method_call(e_this(), "insertVoid", vec![e_var("name"), e_var("attrs")])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("name"), BinOp::StrictEq, e_str("title")),
                vec![
                    s_expr(e_method_call(e_this(), "insertElement", vec![e_str("title"), e_var("attrs")])),
                    s_expr(e_method_call(e_this(), "startRaw", vec![e_str("title"), e_str("rcdata")])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_var("name"), BinOp::StrictEq, e_str("noscript")), BinOp::And, e_binop(e_this_prop("mode"), BinOp::StrictEq, e_str("in head"))),
                vec![
                    s_expr(e_method_call(e_this(), "insertElement", vec![e_str("noscript"), e_var("attrs")])),
                    s_prop_assign(e_this(), "mode", e_str("in head noscript")),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_binop(e_var("name"), BinOp::StrictEq, e_str("noscript")), BinOp::Or, e_binop(e_var("name"), BinOp::StrictEq, e_str("noframes"))), BinOp::Or, e_binop(e_var("name"), BinOp::StrictEq, e_str("style"))),
                vec![
                    s_expr(e_method_call(e_this(), "insertElement", vec![e_var("name"), e_var("attrs")])),
                    s_expr(e_method_call(e_this(), "startRaw", vec![e_var("name"), e_str("rawtext")])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("name"), BinOp::StrictEq, e_str("script")),
                vec![
                    s_expr(e_method_call(e_this(), "insertElement", vec![e_str("script"), e_var("attrs")])),
                    s_expr(e_method_call(e_this(), "startRaw", vec![e_str("script"), e_str("script")])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("name"), BinOp::StrictEq, e_str("template")),
                vec![
                    s_expr(e_method_call(e_this(), "insertElement", vec![e_str("template"), e_var("attrs")])),
                    s_prop_array_push(e_this(), "afe", e_null()),
                    s_prop_assign(e_this(), "framesetOk", e_bool(false)),
                    s_prop_assign(e_this(), "mode", e_str("in template")),
                    s_prop_array_push(e_this(), "templateModes", e_str("in template")),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("name"), BinOp::StrictEq, e_str("head")),
                vec![
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_expr(e_method_call(e_this(), "popOpen", vec![])),
            s_prop_assign(e_this(), "mode", e_str("after head")),
            s_expr(e_method_call(e_this(), "dispatchStart", vec![e_var("name"), e_var("attrs"), e_var("selfClosing")])),
        ])
}

/// `startRaw` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m140() -> MethodBuilder {
    method("startRaw")
        .private()
        .param("tag", TypeExpr::Str)
        .param("mode", TypeExpr::Str)
        .returns(TypeExpr::Void)
        .body(vec![
            s_prop_assign(e_this(), "rawTag", e_var("tag")),
            s_prop_assign(e_this(), "rawMode", e_var("mode")),
            s_prop_assign(e_this(), "origMode", e_this_prop("mode")),
            s_prop_assign(e_this(), "mode", e_str("text")),
        ])
}

/// `startInTable` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m141() -> MethodBuilder {
    method("startInTable")
        .private()
        .param("name", TypeExpr::Str)
        .param("attrs", t_array())
        .param("selfClosing", TypeExpr::Bool)
        .returns(TypeExpr::Void)
        .body(vec![
            s_if(
                e_binop(e_var("name"), BinOp::StrictEq, e_str("caption")),
                vec![
                    s_expr(e_method_call(e_this(), "clearToContext", vec![e_array(vec![e_str("table"), e_str("template"), e_str("html")])])),
                    s_prop_array_push(e_this(), "afe", e_null()),
                    s_expr(e_method_call(e_this(), "insertElement", vec![e_str("caption"), e_var("attrs")])),
                    s_prop_assign(e_this(), "mode", e_str("in caption")),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("name"), BinOp::StrictEq, e_str("colgroup")),
                vec![
                    s_expr(e_method_call(e_this(), "clearToContext", vec![e_array(vec![e_str("table"), e_str("template"), e_str("html")])])),
                    s_expr(e_method_call(e_this(), "insertElement", vec![e_str("colgroup"), e_var("attrs")])),
                    s_prop_assign(e_this(), "mode", e_str("in column group")),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("name"), BinOp::StrictEq, e_str("col")),
                vec![
                    s_expr(e_method_call(e_this(), "clearToContext", vec![e_array(vec![e_str("table"), e_str("template"), e_str("html")])])),
                    s_expr(e_method_call(e_this(), "insertElement", vec![e_str("colgroup"), e_array(vec![])])),
                    s_prop_assign(e_this(), "mode", e_str("in column group")),
                    s_expr(e_method_call(e_this(), "dispatchStart", vec![e_var("name"), e_var("attrs"), e_var("selfClosing")])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_binop(e_var("name"), BinOp::StrictEq, e_str("tbody")), BinOp::Or, e_binop(e_var("name"), BinOp::StrictEq, e_str("tfoot"))), BinOp::Or, e_binop(e_var("name"), BinOp::StrictEq, e_str("thead"))),
                vec![
                    s_expr(e_method_call(e_this(), "clearToContext", vec![e_array(vec![e_str("table"), e_str("template"), e_str("html")])])),
                    s_expr(e_method_call(e_this(), "insertElement", vec![e_var("name"), e_var("attrs")])),
                    s_prop_assign(e_this(), "mode", e_str("in table body")),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_binop(e_var("name"), BinOp::StrictEq, e_str("td")), BinOp::Or, e_binop(e_var("name"), BinOp::StrictEq, e_str("th"))), BinOp::Or, e_binop(e_var("name"), BinOp::StrictEq, e_str("tr"))),
                vec![
                    s_expr(e_method_call(e_this(), "clearToContext", vec![e_array(vec![e_str("table"), e_str("template"), e_str("html")])])),
                    s_expr(e_method_call(e_this(), "insertElement", vec![e_str("tbody"), e_array(vec![])])),
                    s_prop_assign(e_this(), "mode", e_str("in table body")),
                    s_expr(e_method_call(e_this(), "dispatchStart", vec![e_var("name"), e_var("attrs"), e_var("selfClosing")])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("name"), BinOp::StrictEq, e_str("table")),
                vec![
                    s_if(
                        e_not(e_method_call(e_this(), "inScope", vec![e_str("table"), e_str("table")])),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "popUntil", vec![e_str("table")])),
                    s_expr(e_method_call(e_this(), "resetMode", vec![])),
                    s_expr(e_method_call(e_this(), "dispatchStart", vec![e_var("name"), e_var("attrs"), e_var("selfClosing")])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_binop(e_var("name"), BinOp::StrictEq, e_str("style")), BinOp::Or, e_binop(e_var("name"), BinOp::StrictEq, e_str("script"))), BinOp::Or, e_binop(e_var("name"), BinOp::StrictEq, e_str("template"))),
                vec![
                    s_expr(e_method_call(e_this(), "startInHead", vec![e_var("name"), e_var("attrs"), e_var("selfClosing")])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("name"), BinOp::StrictEq, e_str("input")),
                vec![
                    s_assign("type", e_str("")),
                    s_foreach(e_var("attrs"), None, "pair", vec![
                        s_if(
                            e_binop(e_index(e_var("pair"), e_int(0)), BinOp::StrictEq, e_str("type")),
                            vec![
                                s_assign("type", e_call("strtolower", vec![e_index(e_var("pair"), e_int(1))])),
                            ],
                            vec![],
                            None,
                        ),
                    ]),
                    s_if(
                        e_binop(e_var("type"), BinOp::StrictEq, e_str("hidden")),
                        vec![
                            s_expr(e_method_call(e_this(), "insertVoid", vec![e_str("input"), e_var("attrs")])),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("name"), BinOp::StrictEq, e_str("form")),
                vec![
                    s_if(
                        e_binop(e_binop(e_this_prop("formEl"), BinOp::StrictNotEq, e_null()), BinOp::Or, e_method_call(e_this(), "stackHas", vec![e_str("template")])),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "formEl", e_method_call(e_this(), "insertVoid", vec![e_str("form"), e_var("attrs")])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_assign("saved", e_this_prop("foster")),
            s_prop_assign(e_this(), "foster", e_bool(true)),
            s_expr(e_method_call(e_this(), "startInBody", vec![e_var("name"), e_var("attrs"), e_var("selfClosing")])),
            s_prop_assign(e_this(), "foster", e_var("saved")),
        ])
}

/// `startInSelect` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m142() -> MethodBuilder {
    method("startInSelect")
        .private()
        .param("name", TypeExpr::Str)
        .param("attrs", t_array())
        .param("selfClosing", TypeExpr::Bool)
        .returns(TypeExpr::Void)
        .body(vec![
            s_if(
                e_binop(e_var("name"), BinOp::StrictEq, e_str("html")),
                vec![
                    s_expr(e_method_call(e_this(), "startInBody", vec![e_var("name"), e_var("attrs"), e_var("selfClosing")])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("name"), BinOp::StrictEq, e_str("option")),
                vec![
                    s_if(
                        e_binop(e_method_call(e_this(), "currName", vec![]), BinOp::StrictEq, e_str("option")),
                        vec![
                            s_expr(e_method_call(e_this(), "popOpen", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "insertElement", vec![e_str("option"), e_var("attrs")])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("name"), BinOp::StrictEq, e_str("optgroup")),
                vec![
                    s_if(
                        e_binop(e_method_call(e_this(), "currName", vec![]), BinOp::StrictEq, e_str("option")),
                        vec![
                            s_expr(e_method_call(e_this(), "popOpen", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_method_call(e_this(), "currName", vec![]), BinOp::StrictEq, e_str("optgroup")),
                        vec![
                            s_expr(e_method_call(e_this(), "popOpen", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "insertElement", vec![e_str("optgroup"), e_var("attrs")])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("name"), BinOp::StrictEq, e_str("select")),
                vec![
                    s_if(
                        e_not(e_method_call(e_this(), "inScope", vec![e_str("select"), e_str("select")])),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "popUntil", vec![e_str("select")])),
                    s_expr(e_method_call(e_this(), "resetMode", vec![])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_binop(e_var("name"), BinOp::StrictEq, e_str("input")), BinOp::Or, e_binop(e_var("name"), BinOp::StrictEq, e_str("keygen"))), BinOp::Or, e_binop(e_var("name"), BinOp::StrictEq, e_str("textarea"))),
                vec![
                    s_if(
                        e_not(e_method_call(e_this(), "inScope", vec![e_str("select"), e_str("select")])),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "popUntil", vec![e_str("select")])),
                    s_expr(e_method_call(e_this(), "resetMode", vec![])),
                    s_expr(e_method_call(e_this(), "dispatchStart", vec![e_var("name"), e_var("attrs"), e_var("selfClosing")])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_var("name"), BinOp::StrictEq, e_str("script")), BinOp::Or, e_binop(e_var("name"), BinOp::StrictEq, e_str("template"))),
                vec![
                    s_expr(e_method_call(e_this(), "startInHead", vec![e_var("name"), e_var("attrs"), e_var("selfClosing")])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_call("in_array", vec![e_var("name"), e_array(vec![e_str("area"), e_str("br"), e_str("embed"), e_str("hr"), e_str("img"), e_str("keygen"), e_str("wbr"), e_str("param"), e_str("source"), e_str("track"), e_str("col"), e_str("base"), e_str("basefont"), e_str("bgsound"), e_str("link"), e_str("meta"), e_str("frame")]), e_bool(true)]),
                vec![
                    s_expr(e_method_call(e_this(), "insertVoid", vec![e_var("name"), e_var("attrs")])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_expr(e_method_call(e_this(), "insertElement", vec![e_var("name"), e_var("attrs")])),
        ])
}

/// `startInBody` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m143() -> MethodBuilder {
    method("startInBody")
        .private()
        .param("name", TypeExpr::Str)
        .param("attrs", t_array())
        .param("selfClosing", TypeExpr::Bool)
        .returns(TypeExpr::Void)
        .body(vec![
            s_switch(e_var("name"), vec![
                (vec![e_str("html")], vec![
                    s_if(
                        e_method_call(e_this(), "stackHas", vec![e_str("template")]),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_call("count", vec![e_this_prop("open")]), BinOp::Gt, e_int(0)),
                        vec![
                            s_expr(e_method_call(e_this(), "mergeAttributes", vec![e_index(e_this_prop("open"), e_int(0)), e_var("attrs")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return_void(),
                ]),
                (vec![e_str("base"), e_str("basefont"), e_str("bgsound"), e_str("link"), e_str("meta"), e_str("noframes"), e_str("script"), e_str("style"), e_str("template"), e_str("title")], vec![
                    s_expr(e_method_call(e_this(), "startInHead", vec![e_var("name"), e_var("attrs"), e_var("selfClosing")])),
                    s_return_void(),
                ]),
                (vec![e_str("body")], vec![
                    s_if(
                        e_binop(e_binop(e_binop(e_call("count", vec![e_this_prop("open")]), BinOp::Lt, e_int(2)), BinOp::Or, e_binop(e_prop(e_index(e_this_prop("open"), e_int(1)), "localName"), BinOp::StrictNotEq, e_str("body"))), BinOp::Or, e_method_call(e_this(), "stackHas", vec![e_str("template")])),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "framesetOk", e_bool(false)),
                    s_expr(e_method_call(e_this(), "mergeAttributes", vec![e_index(e_this_prop("open"), e_int(1)), e_var("attrs")])),
                    s_return_void(),
                ]),
                (vec![e_str("frameset")], vec![
                    s_if(
                        e_binop(e_binop(e_binop(e_call("count", vec![e_this_prop("open")]), BinOp::Lt, e_int(2)), BinOp::Or, e_binop(e_prop(e_index(e_this_prop("open"), e_int(1)), "localName"), BinOp::StrictNotEq, e_str("body"))), BinOp::Or, e_not(e_this_prop("framesetOk"))),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("body", e_index(e_this_prop("open"), e_int(1))),
                    s_if(
                        e_binop(e_prop(e_var("body"), "parentNode"), BinOp::StrictNotEq, e_null()),
                        vec![
                            s_expr(e_method_call(e_prop(e_var("body"), "parentNode"), "removeChild", vec![e_var("body")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_while(e_binop(e_call("count", vec![e_this_prop("open")]), BinOp::Gt, e_int(1)), vec![
                        s_expr(e_call("array_pop", vec![e_this_prop("open")])),
                    ]),
                    s_expr(e_method_call(e_this(), "insertElement", vec![e_str("frameset"), e_var("attrs")])),
                    s_prop_assign(e_this(), "mode", e_str("in frameset")),
                    s_return_void(),
                ]),
                (vec![e_str("address"), e_str("article"), e_str("aside"), e_str("blockquote"), e_str("center"), e_str("details"), e_str("dialog"), e_str("dir"), e_str("div"), e_str("dl"), e_str("fieldset"), e_str("figcaption"), e_str("figure"), e_str("footer"), e_str("header"), e_str("hgroup"), e_str("main"), e_str("menu"), e_str("nav"), e_str("ol"), e_str("p"), e_str("search"), e_str("section"), e_str("summary"), e_str("ul")], vec![
                    s_expr(e_method_call(e_this(), "closePIfInButtonScope", vec![])),
                    s_expr(e_method_call(e_this(), "insertElement", vec![e_var("name"), e_var("attrs")])),
                    s_return_void(),
                ]),
                (vec![e_str("h1"), e_str("h2"), e_str("h3"), e_str("h4"), e_str("h5"), e_str("h6")], vec![
                    s_expr(e_method_call(e_this(), "closePIfInButtonScope", vec![])),
                    s_assign("c", e_method_call(e_this(), "currName", vec![])),
                    s_if(
                        e_binop(e_binop(e_binop(e_binop(e_call("strlen", vec![e_var("c")]), BinOp::StrictEq, e_int(2)), BinOp::And, e_binop(e_index(e_var("c"), e_int(0)), BinOp::StrictEq, e_str("h"))), BinOp::And, e_binop(e_index(e_var("c"), e_int(1)), BinOp::GtEq, e_str("1"))), BinOp::And, e_binop(e_index(e_var("c"), e_int(1)), BinOp::LtEq, e_str("6"))),
                        vec![
                            s_expr(e_method_call(e_this(), "popOpen", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "insertElement", vec![e_var("name"), e_var("attrs")])),
                    s_return_void(),
                ]),
                (vec![e_str("pre"), e_str("listing")], vec![
                    s_expr(e_method_call(e_this(), "closePIfInButtonScope", vec![])),
                    s_expr(e_method_call(e_this(), "insertElement", vec![e_var("name"), e_var("attrs")])),
                    s_prop_assign(e_this(), "framesetOk", e_bool(false)),
                    s_expr(e_method_call(e_this(), "skipNewline", vec![])),
                    s_return_void(),
                ]),
                (vec![e_str("form")], vec![
                    s_if(
                        e_binop(e_binop(e_this_prop("formEl"), BinOp::StrictNotEq, e_null()), BinOp::And, e_not(e_method_call(e_this(), "stackHas", vec![e_str("template")]))),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "closePIfInButtonScope", vec![])),
                    s_assign("el", e_method_call(e_this(), "insertElement", vec![e_str("form"), e_var("attrs")])),
                    s_if(
                        e_not(e_method_call(e_this(), "stackHas", vec![e_str("template")])),
                        vec![
                            s_prop_assign(e_this(), "formEl", e_var("el")),
                        ],
                        vec![],
                        None,
                    ),
                    s_return_void(),
                ]),
                (vec![e_str("li")], vec![
                    s_prop_assign(e_this(), "framesetOk", e_bool(false)),
                    s_for(Some(s_assign("i", e_binop(e_call("count", vec![e_this_prop("open")]), BinOp::Sub, e_int(1)))), Some(e_binop(e_var("i"), BinOp::GtEq, e_int(0))), Some(s_expr(e_post_dec("i"))), vec![
                        s_assign("n", e_prop(e_index(e_this_prop("open"), e_var("i")), "localName")),
                        s_if(
                            e_binop(e_var("n"), BinOp::StrictEq, e_str("li")),
                            vec![
                                s_expr(e_method_call(e_this(), "generateImpliedEndTags", vec![e_str("li")])),
                                s_expr(e_method_call(e_this(), "popUntil", vec![e_str("li")])),
                                s_break(1),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_binop(e_binop(e_method_call(e_this(), "isSpecialEl", vec![e_index(e_this_prop("open"), e_var("i"))]), BinOp::And, e_binop(e_var("n"), BinOp::StrictNotEq, e_str("address"))), BinOp::And, e_binop(e_var("n"), BinOp::StrictNotEq, e_str("div"))), BinOp::And, e_binop(e_var("n"), BinOp::StrictNotEq, e_str("p"))),
                            vec![
                                s_break(1),
                            ],
                            vec![],
                            None,
                        ),
                    ]),
                    s_expr(e_method_call(e_this(), "closePIfInButtonScope", vec![])),
                    s_expr(e_method_call(e_this(), "insertElement", vec![e_str("li"), e_var("attrs")])),
                    s_return_void(),
                ]),
                (vec![e_str("dd"), e_str("dt")], vec![
                    s_prop_assign(e_this(), "framesetOk", e_bool(false)),
                    s_for(Some(s_assign("i", e_binop(e_call("count", vec![e_this_prop("open")]), BinOp::Sub, e_int(1)))), Some(e_binop(e_var("i"), BinOp::GtEq, e_int(0))), Some(s_expr(e_post_dec("i"))), vec![
                        s_assign("n", e_prop(e_index(e_this_prop("open"), e_var("i")), "localName")),
                        s_if(
                            e_binop(e_binop(e_var("n"), BinOp::StrictEq, e_str("dd")), BinOp::Or, e_binop(e_var("n"), BinOp::StrictEq, e_str("dt"))),
                            vec![
                                s_expr(e_method_call(e_this(), "generateImpliedEndTags", vec![e_var("n")])),
                                s_expr(e_method_call(e_this(), "popUntil", vec![e_var("n")])),
                                s_break(1),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_binop(e_binop(e_method_call(e_this(), "isSpecialEl", vec![e_index(e_this_prop("open"), e_var("i"))]), BinOp::And, e_binop(e_var("n"), BinOp::StrictNotEq, e_str("address"))), BinOp::And, e_binop(e_var("n"), BinOp::StrictNotEq, e_str("div"))), BinOp::And, e_binop(e_var("n"), BinOp::StrictNotEq, e_str("p"))),
                            vec![
                                s_break(1),
                            ],
                            vec![],
                            None,
                        ),
                    ]),
                    s_expr(e_method_call(e_this(), "closePIfInButtonScope", vec![])),
                    s_expr(e_method_call(e_this(), "insertElement", vec![e_var("name"), e_var("attrs")])),
                    s_return_void(),
                ]),
                (vec![e_str("plaintext")], vec![
                    s_expr(e_method_call(e_this(), "closePIfInButtonScope", vec![])),
                    s_expr(e_method_call(e_this(), "insertElement", vec![e_str("plaintext"), e_var("attrs")])),
                    s_prop_assign(e_this(), "rawTag", e_str("plaintext")),
                    s_prop_assign(e_this(), "rawMode", e_str("plaintext")),
                    s_return_void(),
                ]),
                (vec![e_str("button")], vec![
                    s_if(
                        e_method_call(e_this(), "inScope", vec![e_str("button")]),
                        vec![
                            s_expr(e_method_call(e_this(), "generateImpliedEndTags", vec![])),
                            s_expr(e_method_call(e_this(), "popUntil", vec![e_str("button")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "reconstructFormatting", vec![])),
                    s_expr(e_method_call(e_this(), "insertElement", vec![e_str("button"), e_var("attrs")])),
                    s_prop_assign(e_this(), "framesetOk", e_bool(false)),
                    s_return_void(),
                ]),
                (vec![e_str("a")], vec![
                    s_for(Some(s_assign("i", e_binop(e_call("count", vec![e_this_prop("afe")]), BinOp::Sub, e_int(1)))), Some(e_binop(e_var("i"), BinOp::GtEq, e_int(0))), Some(s_expr(e_post_dec("i"))), vec![
                        s_assign("entry", e_index(e_this_prop("afe"), e_var("i"))),
                        s_if(
                            e_binop(e_var("entry"), BinOp::StrictEq, e_null()),
                            vec![
                                s_break(1),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_prop(e_var("entry"), "localName"), BinOp::StrictEq, e_str("a")),
                            vec![
                                s_expr(e_method_call(e_this(), "adoptionAgency", vec![e_str("a")])),
                                s_assign("at", e_method_call(e_this(), "afeIndexOf", vec![e_var("entry")])),
                                s_if(
                                    e_binop(e_var("at"), BinOp::GtEq, e_int(0)),
                                    vec![
                                        s_expr(e_call("array_splice", vec![e_this_prop("afe"), e_var("at"), e_int(1)])),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_expr(e_method_call(e_this(), "removeOpen", vec![e_var("entry")])),
                                s_break(1),
                            ],
                            vec![],
                            None,
                        ),
                    ]),
                    s_expr(e_method_call(e_this(), "reconstructFormatting", vec![])),
                    s_assign("el", e_method_call(e_this(), "insertElement", vec![e_str("a"), e_var("attrs")])),
                    s_expr(e_method_call(e_this(), "afePush", vec![e_var("el"), e_var("attrs")])),
                    s_return_void(),
                ]),
                (vec![e_str("b"), e_str("big"), e_str("code"), e_str("em"), e_str("font"), e_str("i"), e_str("s"), e_str("small"), e_str("strike"), e_str("strong"), e_str("tt"), e_str("u")], vec![
                    s_expr(e_method_call(e_this(), "reconstructFormatting", vec![])),
                    s_assign("el", e_method_call(e_this(), "insertElement", vec![e_var("name"), e_var("attrs")])),
                    s_expr(e_method_call(e_this(), "afePush", vec![e_var("el"), e_var("attrs")])),
                    s_return_void(),
                ]),
                (vec![e_str("nobr")], vec![
                    s_expr(e_method_call(e_this(), "reconstructFormatting", vec![])),
                    s_if(
                        e_method_call(e_this(), "inScope", vec![e_str("nobr")]),
                        vec![
                            s_expr(e_method_call(e_this(), "adoptionAgency", vec![e_str("nobr")])),
                            s_expr(e_method_call(e_this(), "reconstructFormatting", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("el", e_method_call(e_this(), "insertElement", vec![e_str("nobr"), e_var("attrs")])),
                    s_expr(e_method_call(e_this(), "afePush", vec![e_var("el"), e_var("attrs")])),
                    s_return_void(),
                ]),
                (vec![e_str("applet"), e_str("marquee"), e_str("object")], vec![
                    s_expr(e_method_call(e_this(), "reconstructFormatting", vec![])),
                    s_expr(e_method_call(e_this(), "insertElement", vec![e_var("name"), e_var("attrs")])),
                    s_prop_array_push(e_this(), "afe", e_null()),
                    s_prop_assign(e_this(), "framesetOk", e_bool(false)),
                    s_return_void(),
                ]),
                (vec![e_str("table")], vec![
                    s_if(
                        e_not(e_this_prop("quirks")),
                        vec![
                            s_expr(e_method_call(e_this(), "closePIfInButtonScope", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "insertElement", vec![e_str("table"), e_var("attrs")])),
                    s_prop_assign(e_this(), "framesetOk", e_bool(false)),
                    s_prop_assign(e_this(), "mode", e_str("in table")),
                    s_return_void(),
                ]),
                (vec![e_str("area"), e_str("br"), e_str("embed"), e_str("img"), e_str("keygen"), e_str("wbr")], vec![
                    s_expr(e_method_call(e_this(), "reconstructFormatting", vec![])),
                    s_expr(e_method_call(e_this(), "insertVoid", vec![e_var("name"), e_var("attrs")])),
                    s_prop_assign(e_this(), "framesetOk", e_bool(false)),
                    s_return_void(),
                ]),
                (vec![e_str("input")], vec![
                    s_expr(e_method_call(e_this(), "reconstructFormatting", vec![])),
                    s_expr(e_method_call(e_this(), "insertVoid", vec![e_str("input"), e_var("attrs")])),
                    s_assign("type", e_str("")),
                    s_foreach(e_var("attrs"), None, "pair", vec![
                        s_if(
                            e_binop(e_index(e_var("pair"), e_int(0)), BinOp::StrictEq, e_str("type")),
                            vec![
                                s_assign("type", e_call("strtolower", vec![e_index(e_var("pair"), e_int(1))])),
                            ],
                            vec![],
                            None,
                        ),
                    ]),
                    s_if(
                        e_binop(e_var("type"), BinOp::StrictNotEq, e_str("hidden")),
                        vec![
                            s_prop_assign(e_this(), "framesetOk", e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_return_void(),
                ]),
                (vec![e_str("param"), e_str("source"), e_str("track")], vec![
                    s_expr(e_method_call(e_this(), "insertVoid", vec![e_var("name"), e_var("attrs")])),
                    s_return_void(),
                ]),
                (vec![e_str("hr")], vec![
                    s_expr(e_method_call(e_this(), "closePIfInButtonScope", vec![])),
                    s_expr(e_method_call(e_this(), "insertVoid", vec![e_str("hr"), e_var("attrs")])),
                    s_prop_assign(e_this(), "framesetOk", e_bool(false)),
                    s_return_void(),
                ]),
                (vec![e_str("image")], vec![
                    s_expr(e_method_call(e_this(), "startInBody", vec![e_str("img"), e_var("attrs"), e_var("selfClosing")])),
                    s_return_void(),
                ]),
                (vec![e_str("textarea")], vec![
                    s_expr(e_method_call(e_this(), "insertElement", vec![e_str("textarea"), e_var("attrs")])),
                    s_expr(e_method_call(e_this(), "skipNewline", vec![])),
                    s_prop_assign(e_this(), "rawTag", e_str("textarea")),
                    s_prop_assign(e_this(), "rawMode", e_str("rcdata")),
                    s_prop_assign(e_this(), "origMode", e_this_prop("mode")),
                    s_prop_assign(e_this(), "framesetOk", e_bool(false)),
                    s_prop_assign(e_this(), "mode", e_str("text")),
                    s_return_void(),
                ]),
                (vec![e_str("xmp")], vec![
                    s_expr(e_method_call(e_this(), "closePIfInButtonScope", vec![])),
                    s_expr(e_method_call(e_this(), "reconstructFormatting", vec![])),
                    s_prop_assign(e_this(), "framesetOk", e_bool(false)),
                    s_expr(e_method_call(e_this(), "insertElement", vec![e_str("xmp"), e_var("attrs")])),
                    s_expr(e_method_call(e_this(), "startRaw", vec![e_str("xmp"), e_str("rawtext")])),
                    s_return_void(),
                ]),
                (vec![e_str("iframe")], vec![
                    s_prop_assign(e_this(), "framesetOk", e_bool(false)),
                    s_expr(e_method_call(e_this(), "insertElement", vec![e_str("iframe"), e_var("attrs")])),
                    s_expr(e_method_call(e_this(), "startRaw", vec![e_str("iframe"), e_str("rawtext")])),
                    s_return_void(),
                ]),
                (vec![e_str("noembed")], vec![
                    s_expr(e_method_call(e_this(), "insertElement", vec![e_str("noembed"), e_var("attrs")])),
                    s_expr(e_method_call(e_this(), "startRaw", vec![e_str("noembed"), e_str("rawtext")])),
                    s_return_void(),
                ]),
                (vec![e_str("noscript")], vec![
                    s_expr(e_method_call(e_this(), "reconstructFormatting", vec![])),
                    s_expr(e_method_call(e_this(), "insertElement", vec![e_str("noscript"), e_var("attrs")])),
                    s_return_void(),
                ]),
                (vec![e_str("svg")], vec![
                    s_expr(e_method_call(e_this(), "reconstructFormatting", vec![])),
                    s_expr(e_method_call(e_this(), "insertForeign", vec![e_str("svg"), e_var("attrs"), e_self_const("SVG")])),
                    s_if(
                        e_var("selfClosing"),
                        vec![
                            s_expr(e_method_call(e_this(), "popOpen", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return_void(),
                ]),
                (vec![e_str("math")], vec![
                    s_expr(e_method_call(e_this(), "reconstructFormatting", vec![])),
                    s_expr(e_method_call(e_this(), "insertForeign", vec![e_str("math"), e_var("attrs"), e_self_const("MATHML")])),
                    s_if(
                        e_var("selfClosing"),
                        vec![
                            s_expr(e_method_call(e_this(), "popOpen", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return_void(),
                ]),
                (vec![e_str("select")], vec![
                    s_expr(e_method_call(e_this(), "reconstructFormatting", vec![])),
                    s_expr(e_method_call(e_this(), "insertElement", vec![e_str("select"), e_var("attrs")])),
                    s_prop_assign(e_this(), "framesetOk", e_bool(false)),
                    s_if(
                        e_binop(e_binop(e_binop(e_binop(e_binop(e_this_prop("mode"), BinOp::StrictEq, e_str("in table")), BinOp::Or, e_binop(e_this_prop("mode"), BinOp::StrictEq, e_str("in caption"))), BinOp::Or, e_binop(e_this_prop("mode"), BinOp::StrictEq, e_str("in table body"))), BinOp::Or, e_binop(e_this_prop("mode"), BinOp::StrictEq, e_str("in row"))), BinOp::Or, e_binop(e_this_prop("mode"), BinOp::StrictEq, e_str("in cell"))),
                        vec![
                            s_prop_assign(e_this(), "mode", e_str("in select")),
                        ],
                        vec![],
                        Some(vec![
                        s_prop_assign(e_this(), "mode", e_str("in select")),
                    ]),
                    ),
                    s_return_void(),
                ]),
                (vec![e_str("optgroup"), e_str("option")], vec![
                    s_if(
                        e_binop(e_method_call(e_this(), "currName", vec![]), BinOp::StrictEq, e_str("option")),
                        vec![
                            s_expr(e_method_call(e_this(), "popOpen", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "reconstructFormatting", vec![])),
                    s_expr(e_method_call(e_this(), "insertElement", vec![e_var("name"), e_var("attrs")])),
                    s_return_void(),
                ]),
                (vec![e_str("rb"), e_str("rtc")], vec![
                    s_if(
                        e_method_call(e_this(), "inScope", vec![e_str("ruby")]),
                        vec![
                            s_expr(e_method_call(e_this(), "generateImpliedEndTags", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "insertElement", vec![e_var("name"), e_var("attrs")])),
                    s_return_void(),
                ]),
                (vec![e_str("rp"), e_str("rt")], vec![
                    s_if(
                        e_method_call(e_this(), "inScope", vec![e_str("ruby")]),
                        vec![
                            s_expr(e_method_call(e_this(), "generateImpliedEndTags", vec![e_str("rtc")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "insertElement", vec![e_var("name"), e_var("attrs")])),
                    s_return_void(),
                ]),
                (vec![e_str("caption"), e_str("col"), e_str("colgroup"), e_str("frame"), e_str("head"), e_str("tbody"), e_str("td"), e_str("tfoot"), e_str("th"), e_str("thead"), e_str("tr")], vec![
                    s_return_void(),
                ]),
            ], None),
            s_expr(e_method_call(e_this(), "reconstructFormatting", vec![])),
            s_expr(e_method_call(e_this(), "insertElement", vec![e_var("name"), e_var("attrs")])),
            s_if(
                e_var("selfClosing"),
                vec![],
                vec![],
                None,
            ),
        ])
}

/// `skipNewline` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m144() -> MethodBuilder {
    method("skipNewline")
        .private()
        .returns(TypeExpr::Void)
        .body(vec![
            s_if(
                e_binop(e_method_call(e_this(), "peek", vec![]), BinOp::StrictEq, e_str("\n")),
                vec![
                    s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                ],
                vec![],
                None,
            ),
        ])
}

/// `tbEnd` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m145() -> MethodBuilder {
    method("tbEnd")
        .private()
        .param("name", TypeExpr::Str)
        .returns(TypeExpr::Void)
        .body(vec![
            s_expr(e_method_call(e_this(), "flushText", vec![])),
            s_if(
                e_method_call(e_this(), "inForeign", vec![e_str("end"), e_var("name")]),
                vec![
                    s_expr(e_method_call(e_this(), "foreignEnd", vec![e_var("name")])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_expr(e_method_call(e_this(), "dispatchEnd", vec![e_var("name")])),
        ])
}

/// `dispatchEnd` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m146() -> MethodBuilder {
    method("dispatchEnd")
        .private()
        .param("name", TypeExpr::Str)
        .returns(TypeExpr::Void)
        .body(vec![
            s_assign("m", e_this_prop("mode")),
            s_if(
                e_binop(e_var("m"), BinOp::StrictEq, e_str("initial")),
                vec![
                    s_prop_assign(e_this(), "quirks", e_bool(true)),
                    s_prop_assign(e_this(), "mode", e_str("before html")),
                    s_expr(e_method_call(e_this(), "dispatchEnd", vec![e_var("name")])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("m"), BinOp::StrictEq, e_str("before html")),
                vec![
                    s_if(
                        e_call("in_array", vec![e_var("name"), e_array(vec![e_str("head"), e_str("body"), e_str("html"), e_str("br")]), e_bool(true)]),
                        vec![
                            s_expr(e_method_call(e_this(), "createHtml", vec![e_array(vec![])])),
                            s_prop_assign(e_this(), "mode", e_str("before head")),
                            s_expr(e_method_call(e_this(), "dispatchEnd", vec![e_var("name")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("m"), BinOp::StrictEq, e_str("before head")),
                vec![
                    s_if(
                        e_call("in_array", vec![e_var("name"), e_array(vec![e_str("head"), e_str("body"), e_str("html"), e_str("br")]), e_bool(true)]),
                        vec![
                            s_prop_assign(e_this(), "headEl", e_method_call(e_this(), "insertElement", vec![e_str("head"), e_array(vec![])])),
                            s_prop_assign(e_this(), "mode", e_str("in head")),
                            s_expr(e_method_call(e_this(), "dispatchEnd", vec![e_var("name")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("m"), BinOp::StrictEq, e_str("in head")),
                vec![
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("head")),
                        vec![
                            s_expr(e_method_call(e_this(), "popOpen", vec![])),
                            s_prop_assign(e_this(), "mode", e_str("after head")),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_binop(e_var("name"), BinOp::StrictEq, e_str("body")), BinOp::Or, e_binop(e_var("name"), BinOp::StrictEq, e_str("html"))), BinOp::Or, e_binop(e_var("name"), BinOp::StrictEq, e_str("br"))),
                        vec![
                            s_expr(e_method_call(e_this(), "popOpen", vec![])),
                            s_prop_assign(e_this(), "mode", e_str("after head")),
                            s_expr(e_method_call(e_this(), "dispatchEnd", vec![e_var("name")])),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("template")),
                        vec![
                            s_expr(e_method_call(e_this(), "endTemplate", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("m"), BinOp::StrictEq, e_str("in head noscript")),
                vec![
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("noscript")),
                        vec![
                            s_expr(e_method_call(e_this(), "popOpen", vec![])),
                            s_prop_assign(e_this(), "mode", e_str("in head")),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictNotEq, e_str("br")),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "popOpen", vec![])),
                    s_prop_assign(e_this(), "mode", e_str("in head")),
                    s_expr(e_method_call(e_this(), "dispatchEnd", vec![e_var("name")])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("m"), BinOp::StrictEq, e_str("after head")),
                vec![
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("template")),
                        vec![
                            s_expr(e_method_call(e_this(), "endTemplate", vec![])),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_binop(e_var("name"), BinOp::StrictEq, e_str("body")), BinOp::Or, e_binop(e_var("name"), BinOp::StrictEq, e_str("html"))), BinOp::Or, e_binop(e_var("name"), BinOp::StrictEq, e_str("br"))),
                        vec![
                            s_expr(e_method_call(e_this(), "insertElement", vec![e_str("body"), e_array(vec![])])),
                            s_prop_assign(e_this(), "mode", e_str("in body")),
                            s_expr(e_method_call(e_this(), "dispatchEnd", vec![e_var("name")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("m"), BinOp::StrictEq, e_str("text")),
                vec![
                    s_expr(e_method_call(e_this(), "popOpen", vec![])),
                    s_prop_assign(e_this(), "mode", e_this_prop("origMode")),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("m"), BinOp::StrictEq, e_str("in table")),
                vec![
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("table")),
                        vec![
                            s_if(
                                e_not(e_method_call(e_this(), "inScope", vec![e_str("table"), e_str("table")])),
                                vec![
                                    s_return_void(),
                                ],
                                vec![],
                                None,
                            ),
                            s_expr(e_method_call(e_this(), "popUntil", vec![e_str("table")])),
                            s_expr(e_method_call(e_this(), "resetMode", vec![])),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_call("in_array", vec![e_var("name"), e_array(vec![e_str("body"), e_str("caption"), e_str("col"), e_str("colgroup"), e_str("html"), e_str("tbody"), e_str("td"), e_str("tfoot"), e_str("th"), e_str("thead"), e_str("tr")]), e_bool(true)]),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("template")),
                        vec![
                            s_expr(e_method_call(e_this(), "endTemplate", vec![])),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("saved", e_this_prop("foster")),
                    s_prop_assign(e_this(), "foster", e_bool(true)),
                    s_expr(e_method_call(e_this(), "endInBody", vec![e_var("name")])),
                    s_prop_assign(e_this(), "foster", e_var("saved")),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("m"), BinOp::StrictEq, e_str("in caption")),
                vec![
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("caption")),
                        vec![
                            s_if(
                                e_not(e_method_call(e_this(), "inScope", vec![e_str("caption"), e_str("table")])),
                                vec![
                                    s_return_void(),
                                ],
                                vec![],
                                None,
                            ),
                            s_expr(e_method_call(e_this(), "generateImpliedEndTags", vec![])),
                            s_expr(e_method_call(e_this(), "popUntil", vec![e_str("caption")])),
                            s_expr(e_method_call(e_this(), "clearFormattingToMarker", vec![])),
                            s_prop_assign(e_this(), "mode", e_str("in table")),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("table")),
                        vec![
                            s_if(
                                e_not(e_method_call(e_this(), "inScope", vec![e_str("caption"), e_str("table")])),
                                vec![
                                    s_return_void(),
                                ],
                                vec![],
                                None,
                            ),
                            s_expr(e_method_call(e_this(), "generateImpliedEndTags", vec![])),
                            s_expr(e_method_call(e_this(), "popUntil", vec![e_str("caption")])),
                            s_expr(e_method_call(e_this(), "clearFormattingToMarker", vec![])),
                            s_prop_assign(e_this(), "mode", e_str("in table")),
                            s_expr(e_method_call(e_this(), "dispatchEnd", vec![e_var("name")])),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_call("in_array", vec![e_var("name"), e_array(vec![e_str("body"), e_str("col"), e_str("colgroup"), e_str("html"), e_str("tbody"), e_str("td"), e_str("tfoot"), e_str("th"), e_str("thead"), e_str("tr")]), e_bool(true)]),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "endInBody", vec![e_var("name")])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("m"), BinOp::StrictEq, e_str("in column group")),
                vec![
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("colgroup")),
                        vec![
                            s_if(
                                e_binop(e_method_call(e_this(), "currName", vec![]), BinOp::StrictEq, e_str("colgroup")),
                                vec![
                                    s_expr(e_method_call(e_this(), "popOpen", vec![])),
                                    s_prop_assign(e_this(), "mode", e_str("in table")),
                                ],
                                vec![],
                                None,
                            ),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("col")),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("template")),
                        vec![
                            s_expr(e_method_call(e_this(), "endTemplate", vec![])),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_method_call(e_this(), "currName", vec![]), BinOp::StrictEq, e_str("colgroup")),
                        vec![
                            s_expr(e_method_call(e_this(), "popOpen", vec![])),
                            s_prop_assign(e_this(), "mode", e_str("in table")),
                            s_expr(e_method_call(e_this(), "dispatchEnd", vec![e_var("name")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("m"), BinOp::StrictEq, e_str("in table body")),
                vec![
                    s_if(
                        e_binop(e_binop(e_binop(e_var("name"), BinOp::StrictEq, e_str("tbody")), BinOp::Or, e_binop(e_var("name"), BinOp::StrictEq, e_str("tfoot"))), BinOp::Or, e_binop(e_var("name"), BinOp::StrictEq, e_str("thead"))),
                        vec![
                            s_if(
                                e_not(e_method_call(e_this(), "inScope", vec![e_var("name"), e_str("table")])),
                                vec![
                                    s_return_void(),
                                ],
                                vec![],
                                None,
                            ),
                            s_expr(e_method_call(e_this(), "clearToContext", vec![e_array(vec![e_str("tbody"), e_str("tfoot"), e_str("thead"), e_str("template"), e_str("html")])])),
                            s_expr(e_method_call(e_this(), "popOpen", vec![])),
                            s_prop_assign(e_this(), "mode", e_str("in table")),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("table")),
                        vec![
                            s_if(
                                e_not(e_method_call(e_this(), "tableBodyInScope", vec![])),
                                vec![
                                    s_return_void(),
                                ],
                                vec![],
                                None,
                            ),
                            s_expr(e_method_call(e_this(), "clearToContext", vec![e_array(vec![e_str("tbody"), e_str("tfoot"), e_str("thead"), e_str("template"), e_str("html")])])),
                            s_expr(e_method_call(e_this(), "popOpen", vec![])),
                            s_prop_assign(e_this(), "mode", e_str("in table")),
                            s_expr(e_method_call(e_this(), "dispatchEnd", vec![e_var("name")])),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_call("in_array", vec![e_var("name"), e_array(vec![e_str("body"), e_str("caption"), e_str("col"), e_str("colgroup"), e_str("html"), e_str("td"), e_str("th"), e_str("tr")]), e_bool(true)]),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "mode", e_str("in table")),
                    s_expr(e_method_call(e_this(), "dispatchEnd", vec![e_var("name")])),
                    s_prop_assign(e_this(), "mode", e_ternary(e_binop(e_this_prop("mode"), BinOp::StrictEq, e_str("in table")), e_str("in table body"), e_this_prop("mode"))),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("m"), BinOp::StrictEq, e_str("in row")),
                vec![
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("tr")),
                        vec![
                            s_if(
                                e_not(e_method_call(e_this(), "inScope", vec![e_str("tr"), e_str("table")])),
                                vec![
                                    s_return_void(),
                                ],
                                vec![],
                                None,
                            ),
                            s_expr(e_method_call(e_this(), "clearToContext", vec![e_array(vec![e_str("tr"), e_str("template"), e_str("html")])])),
                            s_expr(e_method_call(e_this(), "popOpen", vec![])),
                            s_prop_assign(e_this(), "mode", e_str("in table body")),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("table")),
                        vec![
                            s_if(
                                e_not(e_method_call(e_this(), "inScope", vec![e_str("tr"), e_str("table")])),
                                vec![
                                    s_return_void(),
                                ],
                                vec![],
                                None,
                            ),
                            s_expr(e_method_call(e_this(), "clearToContext", vec![e_array(vec![e_str("tr"), e_str("template"), e_str("html")])])),
                            s_expr(e_method_call(e_this(), "popOpen", vec![])),
                            s_prop_assign(e_this(), "mode", e_str("in table body")),
                            s_expr(e_method_call(e_this(), "dispatchEnd", vec![e_var("name")])),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_binop(e_var("name"), BinOp::StrictEq, e_str("tbody")), BinOp::Or, e_binop(e_var("name"), BinOp::StrictEq, e_str("tfoot"))), BinOp::Or, e_binop(e_var("name"), BinOp::StrictEq, e_str("thead"))),
                        vec![
                            s_if(
                                e_binop(e_not(e_method_call(e_this(), "inScope", vec![e_var("name"), e_str("table")])), BinOp::Or, e_not(e_method_call(e_this(), "inScope", vec![e_str("tr"), e_str("table")]))),
                                vec![
                                    s_return_void(),
                                ],
                                vec![],
                                None,
                            ),
                            s_expr(e_method_call(e_this(), "clearToContext", vec![e_array(vec![e_str("tr"), e_str("template"), e_str("html")])])),
                            s_expr(e_method_call(e_this(), "popOpen", vec![])),
                            s_prop_assign(e_this(), "mode", e_str("in table body")),
                            s_expr(e_method_call(e_this(), "dispatchEnd", vec![e_var("name")])),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_call("in_array", vec![e_var("name"), e_array(vec![e_str("body"), e_str("caption"), e_str("col"), e_str("colgroup"), e_str("html"), e_str("td"), e_str("th")]), e_bool(true)]),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("saved", e_this_prop("foster")),
                    s_prop_assign(e_this(), "foster", e_bool(true)),
                    s_expr(e_method_call(e_this(), "endInBody", vec![e_var("name")])),
                    s_prop_assign(e_this(), "foster", e_var("saved")),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("m"), BinOp::StrictEq, e_str("in cell")),
                vec![
                    s_if(
                        e_binop(e_binop(e_var("name"), BinOp::StrictEq, e_str("td")), BinOp::Or, e_binop(e_var("name"), BinOp::StrictEq, e_str("th"))),
                        vec![
                            s_if(
                                e_not(e_method_call(e_this(), "inScope", vec![e_var("name"), e_str("table")])),
                                vec![
                                    s_return_void(),
                                ],
                                vec![],
                                None,
                            ),
                            s_expr(e_method_call(e_this(), "generateImpliedEndTags", vec![])),
                            s_expr(e_method_call(e_this(), "popUntil", vec![e_var("name")])),
                            s_expr(e_method_call(e_this(), "clearFormattingToMarker", vec![])),
                            s_prop_assign(e_this(), "mode", e_str("in row")),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_call("in_array", vec![e_var("name"), e_array(vec![e_str("body"), e_str("caption"), e_str("col"), e_str("colgroup"), e_str("html")]), e_bool(true)]),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_call("in_array", vec![e_var("name"), e_array(vec![e_str("table"), e_str("tbody"), e_str("tfoot"), e_str("thead"), e_str("tr")]), e_bool(true)]),
                        vec![
                            s_if(
                                e_not(e_method_call(e_this(), "inScope", vec![e_var("name"), e_str("table")])),
                                vec![
                                    s_return_void(),
                                ],
                                vec![],
                                None,
                            ),
                            s_expr(e_method_call(e_this(), "closeCell", vec![])),
                            s_expr(e_method_call(e_this(), "dispatchEnd", vec![e_var("name")])),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "endInBody", vec![e_var("name")])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("m"), BinOp::StrictEq, e_str("in select")),
                vec![
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("optgroup")),
                        vec![
                            s_if(
                                e_binop(e_binop(e_binop(e_method_call(e_this(), "currName", vec![]), BinOp::StrictEq, e_str("option")), BinOp::And, e_binop(e_call("count", vec![e_this_prop("open")]), BinOp::Gt, e_int(1))), BinOp::And, e_binop(e_prop(e_index(e_this_prop("open"), e_binop(e_call("count", vec![e_this_prop("open")]), BinOp::Sub, e_int(2))), "localName"), BinOp::StrictEq, e_str("optgroup"))),
                                vec![
                                    s_expr(e_method_call(e_this(), "popOpen", vec![])),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_binop(e_method_call(e_this(), "currName", vec![]), BinOp::StrictEq, e_str("optgroup")),
                                vec![
                                    s_expr(e_method_call(e_this(), "popOpen", vec![])),
                                ],
                                vec![],
                                None,
                            ),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("option")),
                        vec![
                            s_if(
                                e_binop(e_method_call(e_this(), "currName", vec![]), BinOp::StrictEq, e_str("option")),
                                vec![
                                    s_expr(e_method_call(e_this(), "popOpen", vec![])),
                                ],
                                vec![],
                                None,
                            ),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("select")),
                        vec![
                            s_if(
                                e_not(e_method_call(e_this(), "inScope", vec![e_str("select"), e_str("select")])),
                                vec![
                                    s_return_void(),
                                ],
                                vec![],
                                None,
                            ),
                            s_expr(e_method_call(e_this(), "popUntil", vec![e_str("select")])),
                            s_expr(e_method_call(e_this(), "resetMode", vec![])),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("template")),
                        vec![
                            s_expr(e_method_call(e_this(), "endTemplate", vec![])),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "anyOtherEnd", vec![e_var("name")])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("m"), BinOp::StrictEq, e_str("after body")),
                vec![
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("html")),
                        vec![
                            s_prop_assign(e_this(), "mode", e_str("after after body")),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "mode", e_str("in body")),
                    s_expr(e_method_call(e_this(), "dispatchEnd", vec![e_var("name")])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("m"), BinOp::StrictEq, e_str("after after body")),
                vec![
                    s_prop_assign(e_this(), "mode", e_str("in body")),
                    s_expr(e_method_call(e_this(), "dispatchEnd", vec![e_var("name")])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("m"), BinOp::StrictEq, e_str("in frameset")),
                vec![
                    s_if(
                        e_binop(e_binop(e_var("name"), BinOp::StrictEq, e_str("frameset")), BinOp::And, e_binop(e_method_call(e_this(), "currName", vec![]), BinOp::StrictNotEq, e_str("html"))),
                        vec![
                            s_expr(e_method_call(e_this(), "popOpen", vec![])),
                            s_if(
                                e_binop(e_method_call(e_this(), "currName", vec![]), BinOp::StrictNotEq, e_str("frameset")),
                                vec![
                                    s_prop_assign(e_this(), "mode", e_str("after frameset")),
                                ],
                                vec![],
                                None,
                            ),
                        ],
                        vec![],
                        None,
                    ),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("m"), BinOp::StrictEq, e_str("after frameset")),
                vec![
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("html")),
                        vec![
                            s_prop_assign(e_this(), "mode", e_str("after after frameset")),
                        ],
                        vec![],
                        None,
                    ),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("m"), BinOp::StrictEq, e_str("after after frameset")),
                vec![
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_expr(e_method_call(e_this(), "endInBody", vec![e_var("name")])),
        ])
}

/// `endTemplate` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m147() -> MethodBuilder {
    method("endTemplate")
        .private()
        .returns(TypeExpr::Void)
        .body(vec![
            s_if(
                e_not(e_method_call(e_this(), "stackHas", vec![e_str("template")])),
                vec![
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_expr(e_method_call(e_this(), "generateAllImpliedEndTags", vec![])),
            s_expr(e_method_call(e_this(), "popUntil", vec![e_str("template")])),
            s_expr(e_method_call(e_this(), "clearFormattingToMarker", vec![])),
            s_expr(e_call("array_pop", vec![e_this_prop("templateModes")])),
            s_expr(e_method_call(e_this(), "resetMode", vec![])),
        ])
}

/// `endInBody` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m148() -> MethodBuilder {
    method("endInBody")
        .private()
        .param("name", TypeExpr::Str)
        .returns(TypeExpr::Void)
        .body(vec![
            s_switch(e_var("name"), vec![
                (vec![e_str("template")], vec![
                    s_expr(e_method_call(e_this(), "endTemplate", vec![])),
                    s_return_void(),
                ]),
                (vec![e_str("body")], vec![
                    s_if(
                        e_not(e_method_call(e_this(), "inScope", vec![e_str("body")])),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "mode", e_str("after body")),
                    s_return_void(),
                ]),
                (vec![e_str("html")], vec![
                    s_if(
                        e_not(e_method_call(e_this(), "inScope", vec![e_str("body")])),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "mode", e_str("after body")),
                    s_expr(e_method_call(e_this(), "dispatchEnd", vec![e_str("html")])),
                    s_return_void(),
                ]),
                (vec![e_str("address"), e_str("article"), e_str("aside"), e_str("blockquote"), e_str("button"), e_str("center"), e_str("details"), e_str("dialog"), e_str("dir"), e_str("div"), e_str("dl"), e_str("fieldset"), e_str("figcaption"), e_str("figure"), e_str("footer"), e_str("header"), e_str("hgroup"), e_str("listing"), e_str("main"), e_str("menu"), e_str("nav"), e_str("ol"), e_str("pre"), e_str("search"), e_str("section"), e_str("summary"), e_str("ul")], vec![
                    s_if(
                        e_not(e_method_call(e_this(), "inScope", vec![e_var("name")])),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "generateImpliedEndTags", vec![])),
                    s_expr(e_method_call(e_this(), "popUntil", vec![e_var("name")])),
                    s_return_void(),
                ]),
                (vec![e_str("form")], vec![
                    s_if(
                        e_not(e_method_call(e_this(), "stackHas", vec![e_str("template")])),
                        vec![
                            s_assign("node", e_this_prop("formEl")),
                            s_prop_assign(e_this(), "formEl", e_null()),
                            s_if(
                                e_binop(e_binop(e_binop(e_var("node"), BinOp::StrictEq, e_null()), BinOp::Or, e_not(e_method_call(e_this(), "inScope", vec![e_str("form")]))), BinOp::Or, e_binop(e_method_call(e_this(), "openIndex", vec![e_var("node")]), BinOp::Lt, e_int(0))),
                                vec![
                                    s_return_void(),
                                ],
                                vec![],
                                None,
                            ),
                            s_expr(e_method_call(e_this(), "generateImpliedEndTags", vec![])),
                            s_expr(e_method_call(e_this(), "removeOpen", vec![e_var("node")])),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_not(e_method_call(e_this(), "inScope", vec![e_str("form")])),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "generateImpliedEndTags", vec![])),
                    s_expr(e_method_call(e_this(), "popUntil", vec![e_str("form")])),
                    s_return_void(),
                ]),
                (vec![e_str("p")], vec![
                    s_if(
                        e_not(e_method_call(e_this(), "inScope", vec![e_str("p"), e_str("button")])),
                        vec![
                            s_expr(e_method_call(e_this(), "insertElement", vec![e_str("p"), e_array(vec![])])),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "closeP", vec![])),
                    s_return_void(),
                ]),
                (vec![e_str("li")], vec![
                    s_if(
                        e_not(e_method_call(e_this(), "inScope", vec![e_str("li"), e_str("list")])),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "generateImpliedEndTags", vec![e_str("li")])),
                    s_expr(e_method_call(e_this(), "popUntil", vec![e_str("li")])),
                    s_return_void(),
                ]),
                (vec![e_str("dd"), e_str("dt")], vec![
                    s_if(
                        e_not(e_method_call(e_this(), "inScope", vec![e_var("name")])),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "generateImpliedEndTags", vec![e_var("name")])),
                    s_expr(e_method_call(e_this(), "popUntil", vec![e_var("name")])),
                    s_return_void(),
                ]),
                (vec![e_str("h1"), e_str("h2"), e_str("h3"), e_str("h4"), e_str("h5"), e_str("h6")], vec![
                    s_if(
                        e_not(e_method_call(e_this(), "headingInScope", vec![])),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "generateImpliedEndTags", vec![])),
                    s_expr(e_method_call(e_this(), "popUntilOneOf", vec![e_array(vec![e_str("h1"), e_str("h2"), e_str("h3"), e_str("h4"), e_str("h5"), e_str("h6")])])),
                    s_return_void(),
                ]),
                (vec![e_str("a"), e_str("b"), e_str("big"), e_str("code"), e_str("em"), e_str("font"), e_str("i"), e_str("nobr"), e_str("s"), e_str("small"), e_str("strike"), e_str("strong"), e_str("tt"), e_str("u")], vec![
                    s_if(
                        e_not(e_method_call(e_this(), "adoptionAgency", vec![e_var("name")])),
                        vec![
                            s_expr(e_method_call(e_this(), "anyOtherEnd", vec![e_var("name")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return_void(),
                ]),
                (vec![e_str("applet"), e_str("marquee"), e_str("object")], vec![
                    s_if(
                        e_not(e_method_call(e_this(), "inScope", vec![e_var("name")])),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "generateImpliedEndTags", vec![])),
                    s_expr(e_method_call(e_this(), "popUntil", vec![e_var("name")])),
                    s_expr(e_method_call(e_this(), "clearFormattingToMarker", vec![])),
                    s_return_void(),
                ]),
                (vec![e_str("br")], vec![
                    s_expr(e_method_call(e_this(), "reconstructFormatting", vec![])),
                    s_expr(e_method_call(e_this(), "insertVoid", vec![e_str("br"), e_array(vec![])])),
                    s_prop_assign(e_this(), "framesetOk", e_bool(false)),
                    s_return_void(),
                ]),
            ], None),
            s_expr(e_method_call(e_this(), "anyOtherEnd", vec![e_var("name")])),
        ])
}

/// `anyOtherEnd` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m149() -> MethodBuilder {
    method("anyOtherEnd")
        .private()
        .param("name", TypeExpr::Str)
        .returns(TypeExpr::Void)
        .body(vec![
            s_for(Some(s_assign("i", e_binop(e_call("count", vec![e_this_prop("open")]), BinOp::Sub, e_int(1)))), Some(e_binop(e_var("i"), BinOp::GtEq, e_int(0))), Some(s_expr(e_post_dec("i"))), vec![
                s_assign("node", e_index(e_this_prop("open"), e_var("i"))),
                s_if(
                    e_binop(e_prop(e_var("node"), "localName"), BinOp::StrictEq, e_var("name")),
                    vec![
                        s_expr(e_method_call(e_this(), "generateImpliedEndTags", vec![e_var("name")])),
                        s_while(e_binop(e_call("count", vec![e_this_prop("open")]), BinOp::Gt, e_var("i")), vec![
                            s_expr(e_call("array_pop", vec![e_this_prop("open")])),
                        ]),
                        s_return_void(),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_method_call(e_this(), "isSpecialEl", vec![e_var("node")]),
                    vec![
                        s_return_void(),
                    ],
                    vec![],
                    None,
                ),
            ]),
        ])
}

/// `tbEof` lifted out of `decl_class_dom_elephchtmlparser` so it builds in its own stack frame.
fn decl_class_dom_elephchtmlparser_m150() -> MethodBuilder {
    method("tbEof")
        .private()
        .returns(TypeExpr::Void)
        .body(vec![
            s_expr(e_method_call(e_this(), "flushText", vec![])),
            s_if(
                e_binop(e_this_prop("mode"), BinOp::StrictEq, e_str("text")),
                vec![
                    s_expr(e_method_call(e_this(), "popOpen", vec![])),
                    s_prop_assign(e_this(), "mode", e_this_prop("origMode")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_this_prop("mode"), BinOp::StrictEq, e_str("in head noscript")),
                vec![
                    s_expr(e_method_call(e_this(), "popOpen", vec![])),
                    s_prop_assign(e_this(), "mode", e_str("in head")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_this_prop("mode"), BinOp::StrictEq, e_str("initial")),
                vec![
                    s_prop_assign(e_this(), "quirks", e_bool(true)),
                    s_prop_assign(e_this(), "mode", e_str("before html")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_this_prop("mode"), BinOp::StrictEq, e_str("before html")),
                vec![
                    s_expr(e_method_call(e_this(), "createHtml", vec![e_array(vec![])])),
                    s_prop_assign(e_this(), "mode", e_str("before head")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_this_prop("mode"), BinOp::StrictEq, e_str("before head")),
                vec![
                    s_prop_assign(e_this(), "headEl", e_method_call(e_this(), "insertElement", vec![e_str("head"), e_array(vec![])])),
                    s_prop_assign(e_this(), "mode", e_str("in head")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_this_prop("mode"), BinOp::StrictEq, e_str("in head")),
                vec![
                    s_expr(e_method_call(e_this(), "popOpen", vec![])),
                    s_prop_assign(e_this(), "mode", e_str("after head")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_this_prop("mode"), BinOp::StrictEq, e_str("after head")),
                vec![
                    s_expr(e_method_call(e_this(), "insertElement", vec![e_str("body"), e_array(vec![])])),
                    s_prop_assign(e_this(), "mode", e_str("in body")),
                ],
                vec![],
                None,
            ),
            s_expr(e_method_call(e_this(), "flushText", vec![])),
        ])
}

pub(crate) fn dom_declarations() -> Program {
    internal_declarations(|| {
        vec![
            decl_class_dom_nodelist(),
            decl_class_dom_htmlcollection(),
            decl_class_dom_namednodemap(),
            decl_class_dom_node(),
            decl_class_dom_characterdata(),
            decl_class_dom_text(),
            decl_class_dom_comment(),
            decl_class_dom_documenttype(),
            decl_class_dom_attr(),
            decl_class_dom_element(),
            decl_class_dom_documentfragment(),
            decl_class_dom_document(),
            decl_class_dom_htmldocument(),
            decl_class_dom_elephchtmlparser(),
            decl_fn_elephc_dom_entity_table(),
            decl_fn_elephc_dom_legacy_entities(),
        ]
    })
}
