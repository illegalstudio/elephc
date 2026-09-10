<?php
namespace ReplaceImport;

var_dump(Mb_ErEg_RePlAcE(pattern: "(é)(α)", replacement: "[\\2:\\1]", string: "éα éα"));
var_dump(\mb_eregi_replace("café", "X", "CAFÉ café"));
var_dump(mb_ereg_replace("(?<word>a)|(?<word>b)", "\\0|\\1|\\k<word>", "ab"));
var_dump(mb_ereg_replace("(a)?(b)", "\\1|\\2|\\9", "b"));
echo bin2hex((string)mb_ereg_replace("", "X", "éα")), "\n";
var_dump(mb_ereg_replace("a*", "[\\0]", "aba"));
$replace = mb_ereg_replace(...);
var_dump($replace(string: "aab", replacement: "X", pattern: "a"));
var_dump(call_user_func("mb_eregi_replace", "a", "x", "aAb"));
var_dump(call_user_func_array("mb_ereg_replace", ["string" => "ab", "pattern" => "(a)", "replacement" => "\\1\\1"]));
function marked(string $value): string { echo $value, "\n"; return $value; }
var_dump(mb_ereg_replace(string: marked("abc"), options: marked("i"), replacement: marked("X"), pattern: marked("A")));
mb_regex_encoding("UTF-16LE");
echo bin2hex((string)mb_ereg_replace("(\0a\0)\0", "[\0" . chr(92) . "\0" . "1\0]\0", "a\0b\0")), "\n";
mb_regex_encoding("SJIS");
echo bin2hex((string)mb_ereg_replace("(a)", chr(149) . chr(92) . "\\1", "a")), "\n";
mb_regex_encoding("UTF-8");

class PatternEncoding {
    public function __toString(): string { echo "pattern\n"; mb_regex_encoding("ASCII"); return "a"; }
}
class ReplacementEncoding {
    public function __toString(): string { echo "replacement\n"; mb_regex_encoding("UTF-8"); return "X"; }
}
class SubjectEncoding {
    public function __toString(): string { echo "subject\n"; mb_regex_encoding("UTF-8"); return "éAαa"; }
}
class OptionEncoding {
    public function __toString(): string { echo "options\n"; mb_regex_encoding("ASCII"); return "i"; }
}
mb_regex_encoding("ASCII");
var_dump(mb_ereg_replace(new PatternEncoding(), new ReplacementEncoding(), new SubjectEncoding(), "i"));
mb_regex_encoding("UTF-8");
var_dump(mb_ereg_replace(new PatternEncoding(), new ReplacementEncoding(), new SubjectEncoding(), new OptionEncoding()));
echo mb_regex_encoding(), "\n";
mb_regex_encoding("UTF-8");
mb_ereg_search_init("aab", "a");
var_dump(mb_ereg_search_regs(), mb_ereg_replace("a", "X", "aba"), mb_ereg_search_getregs());
var_dump(mb_eregi_replace("a", "X", "Aa"), mb_ereg_search_getregs());
try { mb_ereg_search(); } catch (\Error $error) { echo $error->getMessage(), "\n"; }
