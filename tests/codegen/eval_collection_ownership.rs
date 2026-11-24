//! Purpose:
//! Checks the heap balance of eval collection builders and traversals.
//!
//! Called from:
//! - The eval codegen test binary.
//!
//! Key details:
//! - Each opaque eval invocation has its own process and leak report.
//! - Results are overwritten and finally dropped before checking the heap.

use crate::support::compile_and_run_with_heap_debug_per_argv;

/// Exercises result construction, traversal, callbacks and mutation with no surviving cell leases.
#[test]
fn test_eval_collection_owners_retire_after_repeated_calls() {
    let cases = [
        ("array_values", "$r = array_values(['k' => 'a', 'b']);"),
        ("array_slice", "$r = array_slice(['a', 'k' => 'b'], 0, null, true);"),
        ("array_reverse", "$r = array_reverse(['a', 'k' => 'b']);"),
        ("array_pad", "$r = array_pad(['a', 'b'], -4, 'x');"),
        ("array_fill", "$r = array_fill(0, 2, 'x');"),
        ("array_fill_keys", "$r = array_fill_keys(['first', 'second'], 'x');"),
        ("array_combine", "$r = array_combine(['first', 'second'], ['a', 'b']);"),
        ("array_column", "$r = array_column([['name' => 'a'], ['other' => 'b']], 'name');"),
        ("array_map", "$r = array_map(function ($v) { return $v; }, ['a', 'b']);"),
        ("array_map multiple", "$r = array_map(function ($a,$b) { return $a; }, ['a','b'], ['c']);"),
        ("array_map zip", "$r = array_map(null, ['a', 'b'], ['c']);"),
        ("array_filter", "$r = array_filter(['a', '', 'b'], function ($v) { return (bool) $v; });"),
        ("array_reduce", "$r = array_reduce([1, 2, 3], function ($carry, $v) { return $carry + $v; }, 0);"),
        ("array_merge", "$r = array_merge(['k' => 'a', 'b'], ['k' => 'c', 'd']);"),
        ("array_diff", "$r = array_diff(['a', 'b'], ['a']);"),
        ("array_diff_key", "$r = array_diff_key(['a' => 1, 'b' => 2], ['a' => 3]);"),
        ("array_unique", "$r = array_unique(['a', 'a', 'b']);"),
        ("array_rand", "$r = array_rand(['a', 'b', 'c'], 2);"),
        ("array_flip", "$r = array_flip(['a', 'b']);"),
        ("array_count_values", "$r = array_count_values(['a', 'a', 'b']);"),
        ("array_sum", "$r = array_sum([1, 2, 3]);"),
        ("array_product", "$r = array_product([1, 2, 3]);"),
        ("in_array", "$r = in_array('b', ['a','b']);"),
        ("array_search miss", "$r = array_search('c', ['a','b']);"),
        ("array_search hit", "$r = array_search('b', ['k' => 'b']);"),
        ("array_pop", "$a = ['a','b']; $r = array_pop($a); unset($a);"),
        ("array_shift", "$a = ['a','b']; $r = array_shift($a); unset($a);"),
        ("array_push", "$a = ['a','b']; $r = array_push($a, 'c'); unset($a);"),
        ("array_unshift", "$a = ['a','b']; $r = array_unshift($a, 'c'); unset($a);"),
        ("array_splice", "$a = ['a','b','c']; $r = array_splice($a, 1, 1, ['d']); unset($a);"),
        ("sort", "$a = ['b','a']; $r = sort($a); unset($a);"),
        ("ksort nested", "$a = ['child' => ['b' => 2, 'a' => 1]]; $r = ksort($a['child'], SORT_STRING); unset($a);"),
        ("usort argument throws", "$a = ['child' => [3,1,2]]; try { usort($a['child'], (function () { throw new Exception('stop'); })()); } catch (Exception $e) { unset($e); } unset($a);"),
        ("array pointer", "$a = ['k' => 'x', 'm' => 'y']; $r = current($a); $r = next($a); unset($a);"),
        ("usort", "$a = [3,1,2]; $r = usort($a, function ($a,$b) { return $a <=> $b; }); unset($a);"),
        ("array_walk", "$a = [1,2]; $r = array_walk($a, function (&$v) { $v += 1; }); unset($a);"),
        ("range", "$r = range(1, 3);"),
        ("str_split", "$r = str_split('abcdef', 2);"),
        ("str_word_count", "$r = str_word_count('alpha beta', 2);"),
        ("count_chars", "$r = count_chars('abbc', 1);"),
        ("parse_url", "$r = parse_url('https://host:8000/path?q=1#fragment');"),
        ("pathinfo", "$r = pathinfo('/tmp/test.txt');"),
        ("scandir", "$r = scandir('.');"),
        ("stat", "$r = stat('.');"),
        ("sscanf", "$r = sscanf('12 word', '%d %s');"),
        ("hash_algos", "$r = hash_algos();"),
        ("stream_get_wrappers", "$r = stream_get_wrappers();"),
        ("get_declared_classes", "$r = get_declared_classes();"),
        ("stream_socket_pair", "$r = stream_socket_pair(STREAM_PF_UNIX, STREAM_SOCK_STREAM, 0); fclose($r[0]); fclose($r[1]);"),
        ("enum cases", "$r = MemoirEvalCase::cases();"),
        ("reflection captures", "$capture = 'x'; $f = function () use ($capture) { return $capture; }; $ref = new ReflectionFunction($f); $r = $ref->getClosureUsedVariables(); unset($ref,$f,$capture);"),
        ("reflection statics", "$ref = new ReflectionFunction('memoir_eval_static'); $r = $ref->getStaticVariables(); unset($ref);"),
    ];
    let sources: Vec<_> = cases.iter().map(|(name, call)| {
        let definitions = match *name {
            "enum cases" => "enum MemoirEvalCase { case A; case B; }",
            "reflection statics" => "function memoir_eval_static() { static $value = 1; }",
            _ => "",
        };
        format!("{definitions} for ($i = 0; $i < 3; $i++) {{ {call} unset($r); }} unset($i); echo 'done';")
    }).collect();
    let args: Vec<_> = sources.iter().map(|source| [source.as_str()]).collect();
    let variants: Vec<_> = args.iter().map(|args| args.as_slice()).collect();
    let results = compile_and_run_with_heap_debug_per_argv("<?php eval($argv[1]);", &variants);
    assert_eq!(results.len(), cases.len());
    let mut failures = Vec::new();
    for ((name, _), result) in cases.into_iter().zip(results) {
        if !result.success || result.stdout != "done"
            || !result.stderr.contains("HEAP DEBUG: leak summary: clean")
        {
            failures.push(format!("{name}: stdout={:?}, stderr={}", result.stdout, result.stderr));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Repeated eval collection and reflection calls retire their intermediate cells.
#[test]
fn test_eval_residual_collection_owners_retire_after_repeated_calls() {
    let class = "class MemoirResidualSubject { public static $s='s'; public function foo(int|string $v) {} }";
    let memory = "$f=fopen('php://memory','w+');";
    let close = "fclose($f); unset($f);";
    let cases = [
        ("iterator keys", "", "$r=iterator_to_array(['x'=>1,'y'=>2],true);", ""),
        ("iterator values", "", "$r=iterator_to_array(['x'=>1,'y'=>2],false);", ""),
        ("strtr", "", "$r=strtr('abz',['a'=>'X','b'=>'Y']);", ""),
        ("vsprintf", "", "$r=vsprintf('%s:%d',['x',1]);", ""),
        ("vprintf", "", "ob_start(); $r=vprintf('%s:%d',['x',1]); ob_end_clean();", ""),
        ("vfprintf", memory, "$r=vfprintf($f,'%s:%d',['x',1]);", close),
        ("fputcsv", memory, "$r=fputcsv($f,['x','y']);", close),
        ("fgetcsv", "$f=fopen('php://memory','w+'); fwrite($f,\"x,y\\n\");", "rewind($f); $r=fgetcsv($f);", close),
        ("metadata", memory, "$r=stream_get_meta_data($f);", close),
        ("stream_select", memory, "$read=[$f]; $write=[]; $except=[]; $r=stream_select($read,$write,$except,0); unset($read,$write,$except);", close),
        ("stream context", "", "$c=stream_context_create(); $r=stream_context_set_option($c,'http','timeout',1); unset($c);", ""),
        ("file", "", "$r=file('data://text/plain,x%0Ay%0A');", ""),
        ("glob", "file_put_contents('memoir-audit.txt','x');", "$r=glob('memoir-*.txt');", "unlink('memoir-audit.txt');"),
        ("iconv get", "", "$r=iconv_get_encoding('all');", ""),
        ("iconv headers", "", "$r=iconv_mime_decode_headers(\"X-Word: one\\r\\nX-Word: two\\r\\n\\r\\n\");", ""),
        ("openssl", "", "$r=openssl_get_cipher_methods();", ""),
        ("callable name", "", "$r=is_callable(['MemoirResidualSubject','foo'],true,$name); unset($name);", ""),
        ("reflection statics", class, "$ref=new ReflectionClass('MemoirResidualSubject'); $r=$ref->getStaticProperties(); unset($ref);", ""),
        ("reflection types", class, "$m=new ReflectionMethod('MemoirResidualSubject','foo'); $parts=$m->getParameters(); $p=$parts[0]; $t=$p->getType(); $r=(string)$t; unset($t,$p,$parts,$m);", ""),
    ];
    let sources: Vec<_> = cases.iter().flat_map(|(_, setup, call, cleanup)| {
        [0, 1, 6].map(|calls| {
            format!("{setup} for ($i=0; $i<{calls}; $i++) {{ {call} unset($r); }} unset($i); {cleanup} echo 'done';")
        })
    }).collect();
    let args: Vec<_> = sources.iter().map(|source| [source.as_str()]).collect();
    let variants: Vec<_> = args.iter().map(|args| args.as_slice()).collect();
    let results = compile_and_run_with_heap_debug_per_argv("<?php eval($argv[1]);", &variants);
    let mut failures = Vec::new();
    for ((case, calls), result) in cases.iter().flat_map(|(name, ..)| [0, 1, 6].map(|calls| (name, calls))).zip(results) {
        if !result.success || result.stdout != "done" || !result.stderr.contains("HEAP DEBUG: leak summary: clean") {
            failures.push(format!("{case} ({calls} calls): stdout={:?}, stderr={}", result.stdout, result.stderr));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
