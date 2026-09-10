<?php
var_dump(\mb_ereg_search_getpos(), \mb_ereg_search_getregs());
var_dump(\MB_ErEg_SeArCh_InIt(string: "猫 aba ab", pattern: "(?<word>a(b)?)(?<empty>)"));
var_dump(\mb_ereg_search_pos());
$saved = \mb_ereg_search_getregs();
var_dump($saved, \mb_ereg_search_getpos());
var_dump(\mb_ereg_search_setpos(offset: 0));
$next = \mb_ereg_search_regs(...);
var_dump($next());
var_dump(call_user_func_array("mb_ereg_search", ["pattern" => null, "options" => null]));
var_dump(\mb_ereg_search_getpos(), \mb_ereg_search_getregs(), $saved);
var_dump(\mb_ereg_search_regs(), \mb_ereg_search_regs(), \mb_ereg_search_getregs());
var_dump(\mb_ereg_search_setpos(-2), \mb_ereg_search_pos(), \mb_ereg_search_getpos());
mb_ereg_search_init("x", "(?<empty>)(x?)");
var_dump(mb_ereg_search_regs(), mb_ereg_search_regs(), mb_ereg_search_getpos());
