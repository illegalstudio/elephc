mod basename;
mod chdir;
mod clearstatcache;
mod copy;
mod dirname;
mod fclose;
mod feof;
mod fgetcsv;
mod fgets;
mod fnmatch;
mod file;
mod file_exists;
mod file_get_contents;
mod file_put_contents;
mod fileatime;
mod filectime;
mod filegroup;
mod fileinode;
mod filemtime;
mod fileowner;
mod fileperms;
mod filesize;
mod filetype;
mod fopen;
mod fputcsv;
mod fread;
mod fseek;
mod ftell;
mod fwrite;
mod getcwd;
mod glob_fn;
mod is_dir;
mod is_executable;
mod is_file;
mod is_link;
mod is_readable;
mod is_writable;
mod mkdir;
mod pathinfo;
mod print_r;
mod readline;
mod realpath;
mod rename;
mod rewind;
mod rmdir;
mod fstat;
mod lstat;
mod scandir;
mod stat;
mod sys_get_temp_dir;
mod tempnam;
mod unlink;
mod var_dump;

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::parser::ast::Expr;
use crate::types::PhpType;

pub fn emit(
    name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    match name {
        "var_dump" => var_dump::emit(name, args, emitter, ctx, data),
        "print_r" => print_r::emit(name, args, emitter, ctx, data),
        "fopen" => fopen::emit(name, args, emitter, ctx, data),
        "fclose" => fclose::emit(name, args, emitter, ctx, data),
        "fread" => fread::emit(name, args, emitter, ctx, data),
        "fwrite" => fwrite::emit(name, args, emitter, ctx, data),
        "fgets" => fgets::emit(name, args, emitter, ctx, data),
        "feof" => feof::emit(name, args, emitter, ctx, data),
        "readline" => readline::emit(name, args, emitter, ctx, data),
        "fseek" => fseek::emit(name, args, emitter, ctx, data),
        "ftell" => ftell::emit(name, args, emitter, ctx, data),
        "rewind" => rewind::emit(name, args, emitter, ctx, data),
        "file_get_contents" => file_get_contents::emit(name, args, emitter, ctx, data),
        "file_put_contents" => file_put_contents::emit(name, args, emitter, ctx, data),
        "file" => file::emit(name, args, emitter, ctx, data),
        "file_exists" => file_exists::emit(name, args, emitter, ctx, data),
        "is_file" => is_file::emit(name, args, emitter, ctx, data),
        "is_dir" => is_dir::emit(name, args, emitter, ctx, data),
        "is_readable" => is_readable::emit(name, args, emitter, ctx, data),
        "is_writable" => is_writable::emit(name, args, emitter, ctx, data),
        "filesize" => filesize::emit(name, args, emitter, ctx, data),
        "filemtime" => filemtime::emit(name, args, emitter, ctx, data),
        "copy" => copy::emit(name, args, emitter, ctx, data),
        "rename" => rename::emit(name, args, emitter, ctx, data),
        "unlink" => unlink::emit(name, args, emitter, ctx, data),
        "mkdir" => mkdir::emit(name, args, emitter, ctx, data),
        "rmdir" => rmdir::emit(name, args, emitter, ctx, data),
        "scandir" => scandir::emit(name, args, emitter, ctx, data),
        "glob" => glob_fn::emit(name, args, emitter, ctx, data),
        "getcwd" => getcwd::emit(name, args, emitter, ctx, data),
        "chdir" => chdir::emit(name, args, emitter, ctx, data),
        "tempnam" => tempnam::emit(name, args, emitter, ctx, data),
        "sys_get_temp_dir" => sys_get_temp_dir::emit(name, args, emitter, ctx, data),
        "fgetcsv" => fgetcsv::emit(name, args, emitter, ctx, data),
        "fputcsv" => fputcsv::emit(name, args, emitter, ctx, data),
        "fileatime" => fileatime::emit(name, args, emitter, ctx, data),
        "filectime" => filectime::emit(name, args, emitter, ctx, data),
        "fileperms" => fileperms::emit(name, args, emitter, ctx, data),
        "fileowner" => fileowner::emit(name, args, emitter, ctx, data),
        "filegroup" => filegroup::emit(name, args, emitter, ctx, data),
        "fileinode" => fileinode::emit(name, args, emitter, ctx, data),
        "filetype" => filetype::emit(name, args, emitter, ctx, data),
        "is_executable" => is_executable::emit(name, args, emitter, ctx, data),
        "is_link" => is_link::emit(name, args, emitter, ctx, data),
        // is_writeable is a documented PHP alias of is_writable.
        "is_writeable" => is_writable::emit(name, args, emitter, ctx, data),
        "clearstatcache" => clearstatcache::emit(name, args, emitter, ctx, data),
        "stat" => stat::emit(name, args, emitter, ctx, data),
        "lstat" => lstat::emit(name, args, emitter, ctx, data),
        "fstat" => fstat::emit(name, args, emitter, ctx, data),
        "basename" => basename::emit(name, args, emitter, ctx, data),
        "dirname" => dirname::emit(name, args, emitter, ctx, data),
        "fnmatch" => fnmatch::emit(name, args, emitter, ctx, data),
        "realpath" => realpath::emit(name, args, emitter, ctx, data),
        "pathinfo" => pathinfo::emit(name, args, emitter, ctx, data),
        _ => None,
    }
}
