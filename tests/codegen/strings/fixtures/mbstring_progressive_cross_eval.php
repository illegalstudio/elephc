<?php
mb_ereg_search_init("éab ab", "(a)(b)");
$source = $argc > 0 ? 'var_dump(mb_ereg_search_pos(), mb_ereg_search_getpos());' : '';
eval($source);
var_dump(mb_ereg_search_getregs());
var_dump(mb_ereg_search(), mb_ereg_search_getpos());
$source = $argc > 0 ? 'var_dump(mb_ereg_search_getregs()); mb_ereg_search_setpos(-2);' : '';
eval($source);
var_dump(mb_ereg_search_pos(), mb_ereg_search_getpos());
