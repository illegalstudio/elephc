//! Purpose:
//! Checks owners retained by user stream-wrapper and filter registrations.
//!
//! Called from:
//! - The executable runtime GC suite and the dedicated fileinfo CI gates.
//!
//! Key details:
//! - Unregistration retires metadata while already-open stream objects stay usable.
//! - Registrations without an explicit removal API retire at normal program shutdown.

use crate::support::*;

/// Registration strings and opened wrapper objects retire on normal and exceptional close paths.
#[test]
fn test_fileinfo_stream_registration_owners() {
    let (out, _) = compile_and_run_with_heap_debug_and_asm(r#"<?php
class FileinfoRegistryWrapper {
    public $context;
    public static int $dropped = 0;
    public static int $closed = 0;
    private int $position = 0;
    private string $payload = "";
    public function stream_open(string $path, string $mode, int $options, ?string &$opened): bool { $this->payload = str_repeat("x", 41); return true; }
    public function stream_stat(): array { return ["mode" => 0100644, "size" => 5]; }
    public function stream_read(int $count): string { $s = substr("alive", $this->position, $count); $this->position += strlen($s); return $s; }
    public function stream_eof(): bool { return $this->position >= 5; }
    public function __destruct() { FileinfoRegistryWrapper::$dropped++; }
}
class FileinfoRegistryClosing extends FileinfoRegistryWrapper {
    public function stream_close(): void { FileinfoRegistryWrapper::$closed++; }
}
class FileinfoRegistryThrowing extends FileinfoRegistryWrapper {
    public function stream_close(): void { FileinfoRegistryWrapper::$closed++; throw new Exception("close"); }
}
class FileinfoRegistryFilter extends php_user_filter {}
for ($i = 0; $i < 40; $i++) {
    $name = "fileinfolease" . $i;
    if (!stream_wrapper_register($name, "FileinfoRegistryWrapper")) { echo "register failed\n"; }
    if (!stream_wrapper_unregister($name)) { echo "unregister failed\n"; }
    if (@stream_wrapper_unregister($name)) { echo "duplicate unregister\n"; }
}
stream_wrapper_register("fileinfoheld", "FileinfoRegistryWrapper");
$stream = fopen("fileinfoheld://x", "rb");
stream_wrapper_unregister("fileinfoheld");
echo stream_get_contents($stream), "\n";
fclose($stream);
stream_wrapper_register("fileinfoclosing", "FileinfoRegistryClosing");
stream_wrapper_register("fileinfothrowing", "FileinfoRegistryThrowing");
for ($i = 0; $i < 8; $i++) {
    $stream = fopen("fileinfoclosing://x", "rb"); fclose($stream);
    $stream = fopen("fileinfothrowing://x", "rb");
    try { fclose($stream); } catch (Exception $e) { if ($e->getMessage() !== "close") { echo "wrong exception\n"; } }
}
echo FileinfoRegistryWrapper::$dropped, ":", FileinfoRegistryWrapper::$closed, "\n";
stream_wrapper_register("fileinfofinal", "FileinfoRegistryWrapper");
stream_filter_register("fileinfo.persist", "FileinfoRegistryFilter");
unset($name, $stream, $e);
echo "done\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "alive\n17:16\ndone\n", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Filter objects release params and properties when creation or close succeeds, refuses or throws.
#[test]
fn test_fileinfo_user_filter_lifecycle_owners() {
    let (out, _) = compile_and_run_with_heap_debug_and_asm(r#"<?php
extern function fcntl(int $descriptor, int $command): int;
function fileinfoOpenDescriptors(): int {
    $count = 0;
    for ($fd = 0; $fd < 128; $fd++) {
        if ((fcntl($fd, 1) & 2147483648) === 0) { $count++; }
    }
    return $count;
}
$initialDescriptorCount = fileinfoOpenDescriptors();
class FileinfoFilterOwner extends php_user_filter {
    public static int $dropped = 0;
    public static int $closed = 0;
    private string $payload = "";
    public function onCreate(): bool { $this->payload = str_repeat("x", 41); return true; }
    public function onClose(): void { if (strlen($this->params["payload"]) < 30) { echo "lost params\n"; } FileinfoFilterOwner::$closed++; }
    public function filter($in, $out, &$consumed, $closing): int { return PSFS_PASS_ON; }
    public function __destruct() { FileinfoFilterOwner::$dropped++; }
}
class FileinfoFilterRefuses extends FileinfoFilterOwner {
    public function onCreate(): bool { return false; }
}
class FileinfoFilterCreateThrows extends FileinfoFilterOwner {
    public function onCreate(): bool { throw new Exception("create"); }
}
class FileinfoFilterCloseThrows extends FileinfoFilterOwner {
    public function onClose(): void { FileinfoFilterOwner::$closed++; throw new Exception("close"); }
}
function fileinfoFilterParams(): mixed { return ["payload" => str_repeat("m", 45)]; }
stream_filter_register("fileinfo.owner", "FileinfoFilterOwner");
stream_filter_register("fileinfo.refuses", "FileinfoFilterRefuses");
stream_filter_register("fileinfo.create.throws", "FileinfoFilterCreateThrows");
stream_filter_register("fileinfo.close.throws", "FileinfoFilterCloseThrows");
for ($i = 0; $i < 8; $i++) {
    foreach ([STREAM_FILTER_READ, STREAM_FILTER_WRITE] as $mode) {
        $stream = fopen("php://temp", "w+b");
        $params = fileinfoFilterParams();
        $filter = stream_filter_append($stream, "fileinfo.owner", $mode, $params);
        unset($params);
        fclose($stream); unset($filter);
    }
    $stream = fopen("php://temp", "w+b");
    $filter = @stream_filter_append($stream, "fileinfo.refuses", STREAM_FILTER_READ, ["payload" => str_repeat("r", 35)]);
    if ($filter !== false) { echo "refusal failed\n"; }
    fclose($stream); unset($filter);
    $stream = fopen("php://temp", "w+b");
    try { stream_filter_append($stream, "fileinfo.create.throws", STREAM_FILTER_READ, fileinfoFilterParams()); }
    catch (Exception $e) { if ($e->getMessage() !== "create") { echo "wrong create error\n"; } }
    fclose($stream);
    $stream = fopen("php://temp", "w+b");
    $filter = stream_filter_append($stream, "fileinfo.close.throws", STREAM_FILTER_READ, ["payload" => str_repeat("c", 39)]);
    try { fclose($stream); } catch (Exception $e) { if ($e->getMessage() !== "close") { echo "wrong close error\n"; } }
    unset($filter);
}
echo FileinfoFilterOwner::$dropped, ":", FileinfoFilterOwner::$closed, "\n";
$stream = fopen("php://temp", "w+b");
$filter = stream_filter_append($stream, "fileinfo.owner", STREAM_FILTER_ALL, ["payload" => str_repeat("a", 43)]);
fclose($stream); unset($filter);
echo "all\n";
echo "descriptors:", fileinfoOpenDescriptors() - $initialDescriptorCount, "\n";
unset($stream, $e);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "40:32\nall\ndescriptors:0\n", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
