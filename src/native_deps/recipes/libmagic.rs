//! Purpose:
//! Builds pinned static libmagic and embeds its complete compiled magic database.
//!
//! Called from:
//! - The curated native package dispatcher for libmagic recipe revision 9.
//!
//! Key details:
//! - The recipe patches upstream 5.46 to match PHP 8.5's bundled libmagic: PHP loads
//!   its own `magic` database and routes libmagic warnings through the PHP error
//!   handler instead of stderr.
//! - A host tool compiles the database for cross targets; libmagic swaps its endianness.
//! - Optional compression and sandbox libraries are disabled explicitly.
//! - Deployed programs need neither a system library nor a database file.

use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::Command;
use crate::codegen_support::platform::Target;
use super::super::{error::{NativeError, NativeErrorKind}, recipe::RecipeRequest, toolchain::run_checked};
use super::util::copy_regular;

const FLAGS: &[&str] = &[
    "--disable-shared", "--enable-static", "--disable-elf", "--disable-elf-core", "--disable-zlib", "--disable-bzlib",
    "--disable-xzlib", "--disable-zstdlib", "--disable-lzlib", "--disable-lrziplib",
    "--disable-libseccomp", "CFLAGS=-O2 -fPIC",
    // Fileinfo does not launch external decompressors, including on iOS.
    "ac_cv_func_fork=no", "ac_cv_func_vfork=no", "ac_cv_func_posix_spawnp=no",
];

/// Replaces one exact upstream snippet, failing closed when the pinned source changes.
fn replace_exact(path: &Path, label: &str, old: &str, new: &str) -> Result<(), NativeError> {
    let text = fs::read_to_string(path)
        .map_err(|e| NativeError::io("read libmagic source for patching", path, e))?;
    // Prefer the original snippet. Only when it is gone do we accept the replacement as
    // proof that a retried build is looking at its own already-patched source.
    if !text.contains(old) {
        if text.contains(new) { return Ok(()); }
        return Err(NativeError::new(
            NativeErrorKind::Build,
            format!("libmagic {label} patch did not match the pinned 5.46 source"),
        )
        .with_path(path));
    }
    fs::write(path, text.replace(old, new))
        .map_err(|e| NativeError::io("write patched libmagic source", path, e))
}

/// Applies PHP 8.5's libmagic compatibility patches before the target build.
///
/// PHP bundles file 5.46 revision 1.355 and patches three upstream behaviors that
/// break its own magic database or leak diagnostics to stderr:
/// - regex patterns with escaped octets are valid and must not be rejected;
/// - duplicate magic entries stay silent;
/// - warnings travel through the PHP error handler via a callback.
fn patch_php_compatibility(source: &Path) -> Result<(), NativeError> {
    replace_exact(
        &source.join("src/funcs.c"),
        "regex validation",
        "		oc = c;
		if (isprint(c) || isspace(c) || c == '\\b'
		    || c == 0x8a) // XXX: apple magic fixme
			continue;
		size_t len = strlen(pat);
		file_magwarn(ms,
		    \"non-ascii characters in regex \\\\%#o `%s'\",
		    c, file_printable(ms, sbuf, sizeof(sbuf), pat, len));
		return -1;",
        "		oc = c;",
    )?;
    replace_exact(
        &source.join("src/apprentice.c"),
        "duplicate warning",
        "		if (x == 0) {
			// Don't warn for DER
			if (mpa.type == FILE_DER)
				return 0;
			file_magwarn1(\"Duplicate magic entry `%s'\",
			    ma->mp->desc);
			file_mdump(ma->mp);
			file_mdump(mb->mp);
			return 0;
		}",
        "		if (x == 0) {
			/* For PHP BC: stay silent like PHP's bundled libmagic. */
			return 0;
		}",
    )?;
    replace_exact(
        &source.join("src/print.c"),
        "warning sink",
        "static void __attribute__((__format__(__printf__, 1, 0)))
file_vmagwarn(const char *f, va_list va)
{
	/* cuz we use stdout for most, stderr here */
	(void) fflush(stdout);

	(void) fprintf(stderr, \"Warning: \");
	(void) vfprintf(stderr, f, va);
	(void) fputc('\\n', stderr);
}",
        "extern void elephc_libmagic_warn(const char *message);
__attribute__((weak)) void
elephc_libmagic_warn(const char *message)
{
	/* Standalone file(1) builds keep upstream's stderr behavior. */
	(void) fflush(stdout);
	(void) fprintf(stderr, \"Warning: %s\\n\", message);
}

static void __attribute__((__format__(__printf__, 1, 0)))
file_vmagwarn(const char *f, va_list va)
{
	va_list copy;
	size_t length;
	char *message;

	va_copy(copy, va);
	length = (size_t)vsnprintf(NULL, 0, f, copy);
	va_end(copy);
	if ((ssize_t)length < 0)
		return;
	if ((message = CAST(char *, malloc(length + 1))) == NULL)
		return;
	(void)vsnprintf(message, length + 1, f, va);
	elephc_libmagic_warn(message);
	free(message);
}",
    )?;
    replace_exact(
        &source.join("src/print.c"),
        "warning limit",
        "	va_list va;

	if (++ms->magwarn == ms->magwarn_max) {
		(void) fprintf(stderr,
		    \"%s, %lu: Maximum number of warnings (%u) exceeded.\\n\",
		    ms->file, CAST(unsigned long, ms->line),
		    ms->magwarn_max);
		(void) fprintf(stderr,
		    \"%s, %lu: Additional warnings are suppressed.\\n\",
		    ms->file, CAST(unsigned long, ms->line));
	}
	if (ms->magwarn >= ms->magwarn_max) {
		return;
	}

	if (ms->file)
		(void) fprintf(stderr, \"%s, %lu: \", ms->file,
		    CAST(unsigned long, ms->line));

	va_start(va, f);",
        "	va_list va;
	(void)ms;

	va_start(va, f);",
    )?;
    Ok(())
}

/// Resolves the managed PCRE2 headers and archives that provide libmagic's regex engine.
///
/// PHP's bundled libmagic compiles magic regexes with PCRE, not the host's POSIX regex.
/// The managed `pcre2` package ships `libpcre2-posix.a`, whose `regcomp`/`regexec` ABI the
/// patched includes expose through a private `regex.h` shim.
fn pcre2_prefixes(request: &RecipeRequest<'_>) -> Result<(std::path::PathBuf, std::path::PathBuf), NativeError> {
    let prefix = request.dependency_prefixes.get("pcre2").ok_or_else(|| {
        NativeError::new(
            NativeErrorKind::Build,
            "libmagic requires its materialized pcre2 dependency".to_string(),
        )
    })?;
    Ok((prefix.join("include"), prefix.join("lib")))
}

/// Exposes PCRE2's POSIX API as the `regex.h` libmagic includes.
fn prepare_regex_shim(request: &RecipeRequest<'_>) -> Result<(std::path::PathBuf, std::path::PathBuf), NativeError> {
    let (include, library) = pcre2_prefixes(request)?;
    let shim = request.staging_prefix.join("build/pcre2-include");
    fs::create_dir_all(&shim)
        .map_err(|e| NativeError::io("create libmagic PCRE2 shim directory", &shim, e))?;
    let header = shim.join("regex.h");
    fs::write(&header, "#include <pcre2posix.h>\n")
        .map_err(|e| NativeError::io("write libmagic PCRE2 regex shim", &header, e))?;
    Ok((include, library))
}

/// Replaces libmagic's POSIX regex wrapper with a length-aware PCRE2 implementation.
///
/// The magic parser decodes `\0` escapes into real NUL bytes and records the pattern
/// length in `m->vallen`. POSIX `regcomp` takes a C string, so PHP's bundled libmagic uses
/// PCRE with an explicit length instead; this patch reproduces that contract.
fn patch_pcre2_regex(source: &Path) -> Result<(), NativeError> {
    // The pinned source carries one trailing space on this line; normalize it so the
    // replacement below (and `git diff --check`) sees whitespace-free text.
    let funcs = source.join("src/funcs.c");
    let text = fs::read_to_string(&funcs)
        .map_err(|e| NativeError::io("read libmagic funcs.c", &funcs, e))?;
    let normalized = text.replace("(%s)\", rc, \n", "(%s)\", rc,\n");
    if normalized != text {
        fs::write(&funcs, normalized)
            .map_err(|e| NativeError::io("normalize libmagic funcs.c", &funcs, e))?;
    }
    replace_exact(
        &source.join("src/file.h"),
        "regex type",
        "typedef regex_t file_regex_t;",
        "typedef struct { void *code; void *match_data; void *match_context; } file_regex_t;",
    )?;
    replace_exact(
        &source.join("src/file.h"),
        "regex declarations",
        "file_protected int file_regcomp(struct magic_set *, file_regex_t *,
    const char *, int);
file_protected int file_regexec(struct magic_set *, file_regex_t *,
    const char *, size_t, regmatch_t *, int);
file_protected void file_regfree(file_regex_t *);",
        "file_protected int file_regcomp(struct magic_set *, file_regex_t *,
    const char *, size_t, int);
file_protected int file_regexec(struct magic_set *, file_regex_t *,
    const char *, size_t, size_t, regmatch_t *, int);
file_protected void file_regfree(file_regex_t *);",
    )?;
    replace_exact(
        &source.join("src/funcs.c"),
        "PCRE2 include",
        "#include \"file.h\"",
        "#include \"file.h\"\n\n#define PCRE2_CODE_UNIT_WIDTH 8\n#include <pcre2.h>",
    )?;
    replace_exact(
        &source.join("src/funcs.c"),
        "regex implementation",
        r#"file_protected int
file_regcomp(struct magic_set *ms file_locale_used, file_regex_t *rx,
    const char *pat, int flags)
{
	if (check_regex(ms, pat) == -1)
		return -1;

#ifdef USE_C_LOCALE
	locale_t old = uselocale(ms->c_lc_ctype);
	assert(old != NULL);
#else
	char old[1024];
	strlcpy(old, setlocale(LC_CTYPE, NULL), sizeof(old));
	(void)setlocale(LC_CTYPE, "C");
#endif
	int rc;
	rc = regcomp(rx, pat, flags);

#ifdef USE_C_LOCALE
	uselocale(old);
#else
	(void)setlocale(LC_CTYPE, old);
#endif
	if (rc > 0 && (ms->flags & MAGIC_CHECK)) {
		char errmsg[512], buf[512];

		(void)regerror(rc, rx, errmsg, sizeof(errmsg));
		file_magerror(ms, "regex error %d for `%s', (%s)", rc,
		    file_printable(ms, buf, sizeof(buf), pat, strlen(pat)),
		    errmsg);
	}
	return rc;
}

/*ARGSUSED*/
file_protected int
file_regexec(struct magic_set *ms file_locale_used, file_regex_t *rx,
    const char *str, size_t nmatch, regmatch_t* pmatch, int eflags)
{
#ifdef USE_C_LOCALE
	locale_t old = uselocale(ms->c_lc_ctype);
	assert(old != NULL);
#else
	char old[1024];
	strlcpy(old, setlocale(LC_CTYPE, NULL), sizeof(old));
	(void)setlocale(LC_CTYPE, "C");
#endif
	int rc;
	/* XXX: force initialization because glibc does not always do this */
	if (nmatch != 0)
		memset(pmatch, 0, nmatch * sizeof(*pmatch));
	rc = regexec(rx, str, nmatch, pmatch, eflags);
#ifdef USE_C_LOCALE
	uselocale(old);
#else
	(void)setlocale(LC_CTYPE, old);
#endif
	return rc;
}

file_protected void
file_regfree(file_regex_t *rx)
{
	regfree(rx);
}"#,
        r#"file_protected int
file_regcomp(struct magic_set *ms, file_regex_t *rx, const char *pat,
    size_t patlen, int flags)
{
	uint32_t options = 0;
	int error;
	PCRE2_SIZE offset;

	rx->code = NULL;
	rx->match_data = NULL;
	rx->match_context = NULL;
	if (check_regex(ms, pat) == -1)
		return -1;
	if (flags & REG_ICASE)
		options |= PCRE2_CASELESS;
	if (flags & REG_NEWLINE)
		options |= PCRE2_MULTILINE;
	if (flags & REG_DOTALL)
		options |= PCRE2_DOTALL;

	rx->code = pcre2_compile(CAST(PCRE2_SPTR, pat), patlen, options,
	    &error, &offset, NULL);
	if (rx->code == NULL) {
		if (ms->flags & MAGIC_CHECK) {
			PCRE2_UCHAR errmsg[256], buf[512];

			(void)pcre2_get_error_message(error, errmsg,
			    sizeof(errmsg));
			file_magerror(ms, "regex error %d for `%s', (%s)",
			    error, file_printable(ms, buf, sizeof(buf), pat,
			    patlen), CAST(const char *, errmsg));
		}
		return -1;
	}
	rx->match_data = pcre2_match_data_create_from_pattern(CAST(pcre2_code *,
	    rx->code), NULL);
	rx->match_context = pcre2_match_context_create(NULL);
	if (rx->match_data == NULL || rx->match_context == NULL) {
		if (rx->match_data != NULL)
			pcre2_match_data_free(CAST(pcre2_match_data *,
			    rx->match_data));
		if (rx->match_context != NULL)
			pcre2_match_context_free(CAST(pcre2_match_context *,
			    rx->match_context));
		pcre2_code_free(CAST(pcre2_code *, rx->code));
		rx->match_data = NULL;
		rx->match_context = NULL;
		rx->code = NULL;
		return -1;
	}
	/* PHP's pcre.* defaults, applied by php_pcre_match_impl. */
	(void)pcre2_set_match_limit(CAST(pcre2_match_context *,
	    rx->match_context), 1000000);
	(void)pcre2_set_depth_limit(CAST(pcre2_match_context *,
	    rx->match_context), 100000);
	return 0;
}

/*ARGSUSED*/
file_protected int
file_regexec(struct magic_set *ms, file_regex_t *rx, const char *str,
    size_t len, size_t nmatch, regmatch_t* pmatch, int eflags)
{
	pcre2_match_data *md = CAST(pcre2_match_data *, rx->match_data);
	pcre2_match_context *mctx = CAST(pcre2_match_context *,
	    rx->match_context);
	uint32_t options = 0;
	int rc;
	(void)ms;

	if (nmatch != 0)
		memset(pmatch, 0, nmatch * sizeof(*pmatch));
	if (md == NULL)
		return REG_ESPACE;
	if (eflags & REG_NOTBOL)
		options |= PCRE2_NOTBOL;
	if (eflags & REG_NOTEOL)
		options |= PCRE2_NOTEOL;
	rc = pcre2_match(CAST(pcre2_code *, rx->code), CAST(PCRE2_SPTR, str),
	    len, 0, options, md, mctx);
	if (rc >= 0) {
		PCRE2_SIZE *ov = pcre2_get_ovector_pointer(md);
		uint32_t count = pcre2_get_ovector_count(md);
		size_t i;

		for (i = 0; i < nmatch && i < count; i++) {
			pmatch[i].rm_so = CAST(regoff_t, ov[2 * i]);
			pmatch[i].rm_eo = CAST(regoff_t, ov[2 * i + 1]);
		}
		rc = 0;
	} else if (rc == PCRE2_ERROR_NOMATCH) {
		rc = REG_NOMATCH;
	} else {
		rc = REG_ASSERT;
	}
	return rc;
}

file_protected void
file_regfree(file_regex_t *rx)
{
	if (rx->match_context != NULL)
		pcre2_match_context_free(CAST(pcre2_match_context *,
		    rx->match_context));
	if (rx->match_data != NULL)
		pcre2_match_data_free(CAST(pcre2_match_data *, rx->match_data));
	if (rx->code != NULL)
		pcre2_code_free(CAST(pcre2_code *, rx->code));
	rx->match_context = NULL;
	rx->match_data = NULL;
	rx->code = NULL;
}"#,
    )?;
    replace_exact(
        &source.join("src/funcs.c"),
        "file_replace regex call",
        "rc = file_regcomp(ms, &rx, pat, REG_EXTENDED);",
        "rc = file_regcomp(ms, &rx, pat, strlen(pat), REG_EXTENDED);",
    )?;
    replace_exact(
        &source.join("src/apprentice.c"),
        "regex validation call",
        "file_regcomp(ms, &rx, m->value.s, REG_EXTENDED);",
        "file_regcomp(ms, &rx, m->value.s, m->vallen, REG_EXTENDED);",
    )?;
    replace_exact(
        &source.join("src/softmagic.c"),
        "format regex call",
        "rc = file_regcomp(ms, &rx, pat, REG_EXTENDED|REG_NOSUB);",
        "rc = file_regcomp(ms, &rx, pat, strlen(pat), REG_EXTENDED|REG_NOSUB);",
    )?;
    replace_exact(
        &source.join("src/softmagic.c"),
        "magic regex call",
        "rc = file_regcomp(ms, rx, m->value.s, REG_EXTENDED | REG_NEWLINE |",
        "rc = file_regcomp(ms, rx, m->value.s, m->vallen, REG_EXTENDED | REG_NEWLINE |",
    )?;
    replace_exact(
        &source.join("src/funcs.c"),
        "file_replace regexec call",
        "while (file_regexec(ms, &rx, ms->o.buf, 1, &rm, 0) == 0) {",
        "while (file_regexec(ms, &rx, ms->o.buf, strlen(ms->o.buf), 1, &rm, 0) == 0) {",
    )?;
    replace_exact(
        &source.join("src/softmagic.c"),
        "format regexec call",
        "rc = file_regexec(ms, &rx, fmt, 0, 0, 0);",
        "rc = file_regexec(ms, &rx, fmt, strlen(fmt), 0, 0, 0);",
    )?;
    replace_exact(
        &source.join("src/softmagic.c"),
        "magic regexec call",
        "rc = file_regexec(ms, rx, RCAST(const char *, search),
		    1, &pmatch, 0);",
        // PHP passes the original buffer and its full length. The local `search` copy is
        // NUL-terminated for POSIX strncmp and drops the last byte, which truncates PCRE2's
        // subject and can turn a non-match into a match.
        "rc = file_regexec(ms, rx, RCAST(const char *, ms->search.s),
		    ms->search.s_len, 1, &pmatch, 0);",
    )?;
    Ok(())
}

/// Builds the host database compiler when needed, then the target archives and database.
pub fn build(request: &RecipeRequest<'_>) -> Result<(), NativeError> {
    let (pcre2_include, pcre2_library) = prepare_regex_shim(request)?;
    let regex_shim = request.staging_prefix.join("build/pcre2-include");
    let build = request.staging_prefix.join("build");
    let library = request.staging_prefix.join("lib");
    let include = request.staging_prefix.join("include");
    for dir in [&build, &library, &include] {
        fs::create_dir_all(dir).map_err(|e| NativeError::io("create libmagic directory", dir, e))?;
    }
    // Warning routing and the non-ASCII regex fix do not need PCRE2, so the host
    // database compiler sees the same accepted entries as the target build.
    patch_php_compatibility(request.source)?;
    let cross = request.target != Target::detect_host();
    let host = request.staging_prefix.join("host-build");
    if cross {
        fs::create_dir_all(&host).map_err(|e| NativeError::io("create libmagic host directory", &host, e))?;
        let mut configure = Command::new("/bin/sh");
        configure.current_dir(&host).arg(request.source.join("configure")).args(FLAGS);
        // Cross compiler variables must not leak into the executable database compiler.
        for variable in ["CC", "CXX", "AR", "RANLIB", "CFLAGS", "CPPFLAGS", "LDFLAGS", "LIBS", "SDKROOT"] {
            configure.env_remove(variable);
        }
        run_checked(&mut configure, "configure libmagic host database compiler")?;
        // Named automake targets skip BUILT_SOURCES, so generate the public header first.
        let mut header = Command::new("make");
        header.current_dir(&host).args(["-C", "src", "magic.h"]);
        run_checked(&mut header, "generate libmagic host header")?;
        let mut make = Command::new("make");
        make.current_dir(&host).args(["-C", "src", "file"]);
        run_checked(&mut make, "build libmagic host database compiler")?;
    }
    // The target additionally needs the length-aware PCRE2 regex implementation.
    patch_pcre2_regex(request.source)?;
    let mut configure = request.toolchain.command(Path::new("/bin/sh"));
    configure.current_dir(&build).arg(request.source.join("configure")).args(FLAGS);
    // libmagic's regex use must resolve to PCRE2's POSIX wrapper, as in PHP's bundled copy.
    configure.env("CPPFLAGS", format!("-I{} -I{} -DPCRE2_STATIC", regex_shim.display(), pcre2_include.display()));
    configure.env("LDFLAGS", format!("-L{}", pcre2_library.display()));
    configure.env("LIBS", "-lpcre2-posix -lpcre2-8");
    if cross { configure.arg(format!("--host={}", request.toolchain.autoconf_host())); }
    run_checked(&mut configure, "configure static libmagic")?;
    let mut header = request.toolchain.command(Path::new("make"));
    header.current_dir(&build).args(["-C", "src", "magic.h"]);
    run_checked(&mut header, "generate libmagic target header")?;
    let mut make = request.toolchain.command(Path::new("make"));
    make.current_dir(&build).args(["-C", "src", if cross { "libmagic.la" } else { "file" }]);
    run_checked(&mut make, "build static libmagic")?;
    let mut database = request.toolchain.command(Path::new("make"));
    database.current_dir(&build).args(["-C", "magic", "magic.mgc"]);
    if cross { database.arg(format!("FILE_COMPILE={}", host.join("src/file").display())); }
    run_checked(&mut database, "compile embedded magic database")?;
    copy_regular("libmagic", &build.join("src/.libs/libmagic.a"), &library.join("libmagic.a"))?;
    copy_regular("libmagic", &build.join("src/magic.h"), &include.join("magic.h"))?;
    embed_database(request, &build.join("magic/magic.mgc"), &build, &include, &library)?;
    fs::remove_dir_all(&build).map_err(|e| NativeError::io("remove libmagic build directory", &build, e))?;
    if cross { fs::remove_dir_all(&host).map_err(|e| NativeError::io("remove libmagic host directory", &host, e))?; }
    Ok(())
}

/// Archives an aligned C array and loader so the database travels with the target binary.
fn embed_database(request: &RecipeRequest<'_>, database: &Path, build: &Path, include: &Path, library: &Path) -> Result<(), NativeError> {
    let bytes = fs::read(database).map_err(|e| NativeError::io("read compiled magic database", database, e))?;
    let source = build.join("database.c");
    let object = build.join("database.o");
    let mut file = fs::File::create(&source).map_err(|e| NativeError::io("create embedded database source", &source, e))?;
    let write = || -> std::io::Result<()> {
        writeln!(file, "#include <magic.h>\n#include <stddef.h>\nstatic const unsigned char database[] __attribute__((aligned(8))) = {{")?;
        for chunk in bytes.chunks(32) {
            for byte in chunk { write!(file, "{byte},")?; }
            writeln!(file)?;
        }
        writeln!(file, "}};\nint elephc_magic_load_default(magic_t m) {{ void *p = (void *)database; size_t n = sizeof(database); return magic_load_buffers(m, &p, &n, 1); }}")
    };
    let mut write = write;
    write().map_err(|e| NativeError::io("write embedded database source", &source, e))?;
    drop(file);
    let mut cc = request.toolchain.command(&request.toolchain.cc);
    cc.args(["-O2", "-fPIC", "-I"]).arg(include).arg("-c").arg(&source).arg("-o").arg(&object);
    run_checked(&mut cc, "compile embedded magic database")?;
    let mut ar = request.toolchain.command(&request.toolchain.ar);
    ar.arg("crs").arg(library.join("libelephc_magic_database.a")).arg(&object);
    run_checked(&mut ar, "archive embedded magic database")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    /// Prevents accidental host-library dependencies or a shared-only artifact.
    #[test]
    fn fileinfo_recipe_is_static_and_self_contained() {
        assert!(FLAGS.contains(&"--enable-static"));
        for flag in ["--disable-zlib", "--disable-bzlib", "--disable-xzlib", "--disable-zstdlib", "--disable-libseccomp"] {
            assert!(FLAGS.contains(&flag));
        }
    }
}
