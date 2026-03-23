mod cstr;
mod feof;
mod fgetcsv;
mod fgets;
mod file;
mod file_get_contents;
mod file_put_contents;
mod fopen;
mod fputcsv;
mod fread;
mod fs;
mod getcwd;
mod glob;
mod scandir;
mod stat;
mod tempnam;

use crate::codegen::emit::Emitter;

pub fn emit_cstr(emitter: &mut Emitter) { cstr::emit_cstr(emitter); }
pub fn emit_fopen(emitter: &mut Emitter) { fopen::emit_fopen(emitter); }
pub fn emit_fgets(emitter: &mut Emitter) { fgets::emit_fgets(emitter); }
pub fn emit_feof(emitter: &mut Emitter) { feof::emit_feof(emitter); }
pub fn emit_fread(emitter: &mut Emitter) { fread::emit_fread(emitter); }
pub fn emit_file_get_contents(emitter: &mut Emitter) { file_get_contents::emit_file_get_contents(emitter); }
pub fn emit_file_put_contents(emitter: &mut Emitter) { file_put_contents::emit_file_put_contents(emitter); }
pub fn emit_file(emitter: &mut Emitter) { file::emit_file(emitter); }
pub fn emit_stat(emitter: &mut Emitter) { stat::emit_stat(emitter); }
pub fn emit_fs(emitter: &mut Emitter) { fs::emit_fs(emitter); }
pub fn emit_getcwd(emitter: &mut Emitter) { getcwd::emit_getcwd(emitter); }
pub fn emit_scandir(emitter: &mut Emitter) { scandir::emit_scandir(emitter); }
pub fn emit_glob(emitter: &mut Emitter) { glob::emit_glob(emitter); }
pub fn emit_tempnam(emitter: &mut Emitter) { tempnam::emit_tempnam(emitter); }
pub fn emit_fgetcsv(emitter: &mut Emitter) { fgetcsv::emit_fgetcsv(emitter); }
pub fn emit_fputcsv(emitter: &mut Emitter) { fputcsv::emit_fputcsv(emitter); }
