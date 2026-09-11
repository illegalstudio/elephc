<?php
namespace SplitImport;

foreach ([-2, -1, 0, 1, 2, 3, 20] as $limit) {
    var_dump(Mb_SpLiT(pattern: "[,;]", string: "a,b;;c,", limit: $limit));
}
var_dump(\mb_split(",", ""), mb_split("", "é,α"), mb_split("a*", "aba"));
$split = mb_split(...);
var_dump($split(string: "red,green,blue", pattern: ",", limit: 2));
var_dump(call_user_func("mb_split", ",", "a,b"));
var_dump(call_user_func_array("mb_split", ["limit" => 2, "string" => "a,b,c", "pattern" => ","]));
mb_regex_encoding("UTF-16LE");
var_dump(mb_split(",\0", "a\0,\0b\0"));
mb_regex_encoding("BIG5");
$encoded_fields = mb_split(",", chr(164) . chr(164) . "," . chr(164) . chr(229));
if (is_array($encoded_fields)) {
    foreach ($encoded_fields as $field) { echo bin2hex((string)$field), "\n"; }
}
echo mb_internal_encoding("BIG5") ? mb_internal_encoding() : "wrong", "\n";
mb_internal_encoding("UTF-8");
mb_regex_encoding("UTF-8");

class SplitPattern {
    public function __toString(): string { echo "pattern\n"; mb_regex_encoding("ASCII"); return "a"; }
}
class SplitSubject {
    public function __toString(): string { echo "subject\n"; mb_regex_encoding("UTF-8"); mb_regex_set_options("ir"); return "éAαa"; }
}
var_dump(mb_split(new SplitPattern(), new SplitSubject(), "2"));
mb_regex_set_options("pr");
mb_ereg_search_init("aab", "a");
var_dump(mb_ereg_search_regs());
var_dump(mb_split("a", "aba", 1), mb_ereg_search_getregs());
mb_regex_set_options("ir");
var_dump(mb_split("a", "Aa", 0), mb_ereg_search_getregs());
try { mb_ereg_search(); } catch (\Error $error) { echo $error->getMessage(), "\n"; }
