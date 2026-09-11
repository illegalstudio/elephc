<?php
function show_regex_error(Throwable $error): void {
    echo get_class($error), ":", $error->getMessage(), "\n";
    $previous = $error->getPrevious();
    if ($previous !== null) { show_regex_error($previous); }
}
try { mb_ereg_search(options: "iQ"); } catch (Throwable $e) { show_regex_error($e); }
var_dump(mb_ereg_search_getpos(), mb_ereg_search_getregs());
try { mb_ereg_search_regs("a", "Q"); } catch (Throwable $e) { show_regex_error($e); }
try { mb_ereg_search_init("xx", "", "Q"); } catch (Throwable $e) { show_regex_error($e); }
mb_ereg_search_init("abéabb", "(a)(b*)");
try { mb_ereg_search_pos(null, "iQ"); } catch (Throwable $e) { show_regex_error($e); }
var_dump(mb_ereg_search_getpos(), mb_ereg_search_getregs());
try { mb_ereg_search_setpos(100); } catch (Throwable $e) { show_regex_error($e); }
var_dump(mb_ereg_search_getpos());
var_dump(mb_ereg_search("["), mb_ereg_search_getregs());
$init = $argc > 0 ? "mb_ereg_search_init" : "mb_strlen";
try { $init(); } catch (ArgumentCountError $e) { echo $e->getMessage(), "\n"; }
try { $init([]); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
