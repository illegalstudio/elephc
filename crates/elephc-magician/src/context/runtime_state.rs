//! Purpose:
//! Manages mutable eval runtime state, execution scopes, resources, and call-site metadata.
//!
//! Called from:
//! - Interpreter execution and builtins requiring process-level eval state.
//!
//! Key details:
//! - Static cells, include state, scope stacks, errors, timezone, HTTP status, and magic paths live here.

use super::*;

impl ElephcEvalContext {
    /// Returns true when the context has a dynamic or native function with this lowercase PHP name.
    pub fn has_function(&self, name: &str) -> bool {
        self.functions.contains_key(name) || self.native_functions.contains_key(name)
    }

    /// Returns true when the context has a closure registered under this synthetic name.
    pub fn has_closure(&self, name: &str) -> bool {
        self.closures.contains_key(name)
    }

    /// Returns a stored static local cell for an eval-declared function.
    pub fn static_local(&self, function_name: &str, name: &str) -> Option<RuntimeCellHandle> {
        self.static_locals
            .get(&(function_name.to_string(), name.to_string()))
            .copied()
    }

    /// Stores one static local cell and returns any replaced distinct cell.
    pub fn set_static_local(
        &mut self,
        function_name: impl Into<String>,
        name: impl Into<String>,
        cell: RuntimeCellHandle,
    ) -> Option<RuntimeCellHandle> {
        let previous = self
            .static_locals
            .insert((function_name.into(), name.into()), cell);
        previous.filter(|previous| *previous != cell)
    }

    /// Returns a stored static property cell for an eval-declared class.
    pub fn static_property(&self, class_name: &str, name: &str) -> Option<RuntimeCellHandle> {
        self.static_properties
            .get(&(normalize_class_name(class_name), name.to_string()))
            .copied()
    }

    /// Stores one eval static property cell and returns any replaced distinct cell.
    pub fn set_static_property(
        &mut self,
        class_name: &str,
        name: impl Into<String>,
        cell: RuntimeCellHandle,
    ) -> Option<RuntimeCellHandle> {
        let previous = self
            .static_properties
            .insert((normalize_class_name(class_name), name.into()), cell);
        previous.filter(|previous| *previous != cell)
    }

    /// Binds one eval static property slot to a persistent PHP reference target.
    pub fn bind_static_property_alias(
        &mut self,
        class_name: &str,
        name: &str,
        target: EvalReferenceTarget,
    ) -> Option<EvalReferenceTarget> {
        self.static_property_aliases
            .insert((normalize_class_name(class_name), name.to_string()), target)
    }

    /// Returns the persistent reference target bound to one eval static property slot.
    pub fn static_property_alias(
        &self,
        class_name: &str,
        name: &str,
    ) -> Option<&EvalReferenceTarget> {
        self.static_property_aliases
            .get(&(normalize_class_name(class_name), name.to_string()))
    }

    /// Returns a materialized eval class constant cell.
    pub fn class_constant_cell(&self, class_name: &str, name: &str) -> Option<RuntimeCellHandle> {
        self.class_constants
            .get(&(normalize_class_name(class_name), name.to_string()))
            .copied()
    }

    /// Stores one eval class constant cell and returns any replaced distinct cell.
    pub fn set_class_constant_cell(
        &mut self,
        class_name: &str,
        name: impl Into<String>,
        cell: RuntimeCellHandle,
    ) -> Option<RuntimeCellHandle> {
        let previous = self
            .class_constants
            .insert((normalize_class_name(class_name), name.into()), cell);
        previous.filter(|previous| *previous != cell)
    }

    /// Returns true when an eval include key was already loaded by this context.
    pub fn has_included_file(&self, path: &str) -> bool {
        self.included_files.contains(path)
    }

    /// Records one successfully loaded eval include key for include_once/require_once.
    pub fn mark_included_file(&mut self, path: impl Into<String>) {
        self.included_files.insert(path.into());
    }

    /// Records PHP-visible permission bits against the file's stable identity.
    #[cfg(any(windows, test))]
    pub(crate) fn remember_local_file_mode(
        &mut self,
        path: impl AsRef<std::path::Path>,
        mode: u32,
    ) {
        if let Some(key) = local_file_mode_key(path.as_ref()) {
            self.local_file_modes.insert(key, mode & 0o7777);
        }
    }

    /// Returns emulated PHP permission bits for one local file, when present.
    #[cfg(any(windows, test))]
    pub(crate) fn local_file_mode(&self, path: impl AsRef<std::path::Path>) -> Option<u32> {
        let key = local_file_mode_key(path.as_ref())?;
        self.local_file_modes.get(&key).copied()
    }

    /// Captures emulated mode state before rename/copy/link/unlink mutates paths.
    #[cfg(any(windows, test))]
    pub(crate) fn capture_local_file_mode(
        &self,
        path: impl AsRef<std::path::Path>,
    ) -> Option<LocalFileModeToken> {
        let path = path.as_ref();
        let key = local_file_mode_key(path)?;
        let mode = self.local_file_modes.get(&key).copied()?;
        Some(LocalFileModeToken {
            key,
            mode,
            last_link: local_file_link_count(path).is_none_or(|count| count <= 1),
        })
    }

    /// Copies captured mode state onto a newly materialized destination file.
    #[cfg(any(windows, test))]
    pub(crate) fn copy_local_file_mode(
        &mut self,
        source: Option<&LocalFileModeToken>,
        replaced_destination: Option<&LocalFileModeToken>,
        destination: impl AsRef<std::path::Path>,
    ) {
        if let Some(replaced) = replaced_destination.filter(|token| token.last_link) {
            self.local_file_modes.remove(&replaced.key);
        }
        if let Some(source) = source {
            self.remember_local_file_mode(destination, source.mode);
        }
    }

    /// Moves captured mode state from the old path identity to the renamed file.
    #[cfg(any(windows, test))]
    pub(crate) fn rename_local_file_mode(
        &mut self,
        source: Option<LocalFileModeToken>,
        replaced_destination: Option<LocalFileModeToken>,
        destination: impl AsRef<std::path::Path>,
    ) {
        if let Some(replaced) = replaced_destination.filter(|token| token.last_link) {
            self.local_file_modes.remove(&replaced.key);
        }
        if let Some(source) = source {
            self.local_file_modes.remove(&source.key);
            self.remember_local_file_mode(destination, source.mode);
        }
    }

    /// Purges captured mode state only when unlink removed the inode's last alias.
    #[cfg(any(windows, test))]
    pub(crate) fn unlink_local_file_mode(&mut self, removed: Option<LocalFileModeToken>) {
        if let Some(removed) = removed.filter(|token| token.last_link) {
            self.local_file_modes.remove(&removed.key);
        }
    }

    /// Stores the non-owned global scope handle used by eval `global` aliases.
    pub fn set_global_scope(&mut self, scope: *mut ElephcEvalScope) -> bool {
        if scope.is_null() {
            self.global_scope = None;
            false
        } else {
            self.global_scope = Some(scope);
            true
        }
    }

    /// Returns the non-owned global scope handle for eval `global` aliases.
    pub fn global_scope_ptr(&self) -> Option<*mut ElephcEvalScope> {
        self.global_scope
    }

    /// Pushes an eval-executed function name for magic-constant resolution.
    pub fn push_function(&mut self, name: impl Into<String>) {
        self.function_stack.push(name.into());
    }

    /// Pops the current eval-executed function name after its body completes.
    pub fn pop_function(&mut self) {
        self.function_stack.pop();
    }

    /// Returns the current eval-executed function name, if execution is inside one.
    pub fn current_function(&self) -> Option<&str> {
        self.function_stack.last().map(String::as_str)
    }

    /// Pushes the eval class whose method is currently executing.
    pub fn push_class_scope(&mut self, name: impl Into<String>) {
        self.class_stack.push(name.into());
    }

    /// Pops the current eval class method scope.
    pub fn pop_class_scope(&mut self) {
        self.class_stack.pop();
    }

    /// Returns the current eval class scope, if execution is inside a method.
    pub fn current_class_scope(&self) -> Option<&str> {
        self.class_stack.last().map(String::as_str)
    }

    /// Pushes the class name used to dispatch the current eval method call.
    pub fn push_called_class_scope(&mut self, name: impl Into<String>) {
        self.called_class_stack.push(name.into());
    }

    /// Pops the current late-static-bound eval class scope.
    pub fn pop_called_class_scope(&mut self) {
        self.called_class_stack.pop();
    }

    /// Returns the current late-static-bound eval class scope, if execution is inside a method.
    pub fn current_called_class_scope(&self) -> Option<&str> {
        self.called_class_stack.last().map(String::as_str)
    }

    /// Returns a dynamic called-class override for a generated/AOT frame entering eval.
    pub fn native_frame_called_class_override(
        &self,
        class_name: &str,
        called_class_name: &str,
    ) -> Option<String> {
        let class_name = class_name.trim_start_matches('\\');
        let called_class_name = called_class_name.trim_start_matches('\\');
        if class_name.is_empty() || !called_class_name.eq_ignore_ascii_case(class_name) {
            return None;
        }
        if let Some(called_class) =
            native_frame_called_class_override(class_name, called_class_name)
        {
            return Some(called_class);
        }
        let active = self.current_called_class_scope()?.trim_start_matches('\\');
        if active.is_empty() || active.eq_ignore_ascii_case(class_name) {
            return None;
        }
        let active = self
            .resolve_class_name(active)
            .unwrap_or_else(|| active.to_string());
        self.class_parent_names(&active)
            .iter()
            .any(|parent| parent.eq_ignore_ascii_case(class_name))
            .then_some(active)
    }

    /// Pushes PHP-visible method magic constants for the current eval method frame.
    pub fn push_method_magic_scope(&mut self, class_name: &str, method: &EvalClassMethod) {
        self.magic_stack.push(EvalMagicScope {
            function_name: method.magic_function_name().to_string(),
            method_name: method.magic_method_name(class_name),
            class_name: class_name.trim_start_matches('\\').to_string(),
            trait_name: method
                .trait_origin()
                .map(|trait_name| trait_name.trim_start_matches('\\').to_string())
                .unwrap_or_default(),
        });
    }

    /// Pushes PHP-visible class-like member magic constants for default expressions.
    pub fn push_class_like_member_magic_scope(
        &mut self,
        class_name: &str,
        trait_name: Option<&str>,
    ) {
        self.magic_stack.push(EvalMagicScope {
            function_name: String::new(),
            method_name: String::new(),
            class_name: class_name.trim_start_matches('\\').to_string(),
            trait_name: trait_name
                .map(|trait_name| trait_name.trim_start_matches('\\').to_string())
                .unwrap_or_default(),
        });
    }

    /// Pushes PHP-visible callable magic constants for reflected parameter defaults.
    pub fn push_callable_magic_scope(
        &mut self,
        function_name: &str,
        method_name: &str,
        class_name: Option<&str>,
        trait_name: Option<&str>,
    ) {
        self.magic_stack.push(EvalMagicScope {
            function_name: function_name.to_string(),
            method_name: method_name.to_string(),
            class_name: class_name
                .map(|class_name| class_name.trim_start_matches('\\').to_string())
                .unwrap_or_default(),
            trait_name: trait_name
                .map(|trait_name| trait_name.trim_start_matches('\\').to_string())
                .unwrap_or_default(),
        });
    }

    /// Pops the current PHP-visible eval magic-constant scope.
    pub fn pop_magic_scope(&mut self) {
        self.magic_stack.pop();
    }

    /// Returns the PHP `__FUNCTION__` value for the current eval frame.
    pub fn current_magic_function(&self) -> Option<&str> {
        self.magic_stack
            .last()
            .map(|scope| scope.function_name.as_str())
    }

    /// Returns the PHP `__METHOD__` value for the current eval method frame.
    pub fn current_magic_method(&self) -> Option<&str> {
        self.magic_stack
            .last()
            .map(|scope| scope.method_name.as_str())
    }

    /// Returns the PHP `__CLASS__` value for the current eval method frame.
    pub fn current_magic_class(&self) -> Option<&str> {
        self.magic_stack
            .last()
            .map(|scope| scope.class_name.as_str())
    }

    /// Returns the PHP `__TRAIT__` value for the current eval method frame.
    pub fn current_magic_trait(&self) -> Option<&str> {
        self.magic_stack
            .last()
            .map(|scope| scope.trait_name.as_str())
    }

    /// Captures the current eval execution stacks for later caller-context-sensitive work.
    pub fn execution_scope(&self) -> ElephcEvalExecutionScope {
        ElephcEvalExecutionScope {
            function_stack: self.function_stack.clone(),
            class_stack: self.class_stack.clone(),
            called_class_stack: self.called_class_stack.clone(),
        }
    }

    /// Replaces eval execution stacks and returns the previous stacks for restoration.
    pub fn replace_execution_scope(
        &mut self,
        scope: ElephcEvalExecutionScope,
    ) -> ElephcEvalExecutionScope {
        ElephcEvalExecutionScope {
            function_stack: std::mem::replace(&mut self.function_stack, scope.function_stack),
            class_stack: std::mem::replace(&mut self.class_stack, scope.class_stack),
            called_class_stack: std::mem::replace(
                &mut self.called_class_stack,
                scope.called_class_stack,
            ),
        }
    }

    /// Records a Throwable cell that escaped from an eval-executed function call.
    pub fn set_pending_throw(&mut self, value: RuntimeCellHandle) {
        self.pending_throw = Some(value);
    }

    /// Returns and clears the Throwable cell currently escaping through eval.
    pub fn take_pending_throw(&mut self) -> Option<RuntimeCellHandle> {
        self.pending_throw.take()
    }

    /// Returns the eval-local SPL autoload extension list.
    pub fn spl_autoload_extensions(&self) -> &str {
        &self.spl_autoload_extensions
    }

    /// Replaces the eval-local SPL autoload extension list.
    pub fn set_spl_autoload_extensions(&mut self, extensions: impl Into<String>) {
        self.spl_autoload_extensions = extensions.into();
    }

    /// Returns the eval-local stream resource table.
    pub(crate) fn stream_resources(&self) -> &EvalStreamResources {
        &self.streams
    }

    /// Returns mutable access to the eval-local stream resource table.
    pub(crate) fn stream_resources_mut(&mut self) -> &mut EvalStreamResources {
        &mut self.streams
    }

    /// Clears the eval-local JSON error state after a successful JSON operation.
    pub fn clear_json_error(&mut self) {
        self.json_last_error = 0;
        self.json_last_error_msg.clear();
        self.json_last_error_msg.push_str("No error");
    }

    /// Records the eval-local JSON error state for `json_last_error*()` calls.
    pub fn set_json_error(&mut self, code: i64, message: impl Into<String>) {
        self.json_last_error = code;
        self.json_last_error_msg = message.into();
    }

    /// Returns the PHP `JSON_ERROR_*` code for the last eval JSON operation.
    pub const fn json_last_error(&self) -> i64 {
        self.json_last_error
    }

    /// Returns the PHP message for the last eval JSON operation.
    pub fn json_last_error_msg(&self) -> &str {
        &self.json_last_error_msg
    }

    /// Returns the eval-local PHP default timezone identifier.
    pub fn default_timezone(&self) -> &str {
        &self.default_timezone
    }

    /// Replaces the eval-local PHP default timezone identifier.
    pub fn set_default_timezone(&mut self, timezone: impl Into<String>) {
        self.default_timezone = timezone.into();
    }

    /// Returns the eval-local HTTP response code used by web-facing builtins.
    pub const fn http_response_code(&self) -> i64 {
        self.http_response_code
    }

    /// Applies a new eval-local HTTP response code and returns the previous one.
    pub fn replace_http_response_code(&mut self, response_code: i64) -> i64 {
        let previous = self.http_response_code;
        if response_code > 0 {
            self.http_response_code = response_code;
        }
        previous
    }

    /// Updates the source file, directory, and line for the current eval call site.
    pub fn set_call_site(&mut self, file: impl Into<String>, dir: impl Into<String>, line: i64) {
        self.call_file = file.into();
        self.call_dir = dir.into();
        self.call_line = line;
        self.file_magic_override = None;
    }

    /// Returns a copy of the current call-site metadata for temporary overrides.
    pub fn call_site(&self) -> (String, String, i64, Option<String>) {
        (
            self.call_file.clone(),
            self.call_dir.clone(),
            self.call_line,
            self.file_magic_override.clone(),
        )
    }

    /// Overrides `__FILE__` while executing an actual file through eval include.
    pub fn set_file_magic_override(&mut self, file: Option<String>) {
        self.file_magic_override = file;
    }

    /// Returns the source directory associated with the current eval call site.
    pub fn call_dir(&self) -> &str {
        &self.call_dir
    }

    /// Returns PHP's `__FILE__` string for code currently running inside eval.
    pub fn eval_file_magic(&self) -> String {
        if let Some(file) = &self.file_magic_override {
            return file.clone();
        }
        if self.call_file.is_empty() {
            return String::new();
        }
        format!("{}({}) : eval()'d code", self.call_file, self.call_line)
    }
}

/// Builds an inode-like key, falling back to a normalized absolute path.
#[cfg(any(windows, test))]
fn local_file_mode_key(path: &std::path::Path) -> Option<LocalFileModeKey> {
    let metadata = std::fs::metadata(path).ok()?;
    if let Some((volume, index)) = local_file_identity(path, &metadata) {
        return Some(LocalFileModeKey::FileId { volume, index });
    }
    Some(LocalFileModeKey::Path(normalize_local_file_mode_path(path)?))
}

/// Returns a stable host filesystem identity for hard-link-aware mode tracking.
#[cfg(any(windows, test))]
fn local_file_identity(
    _path: &std::path::Path,
    metadata: &std::fs::Metadata,
) -> Option<(u64, u64)> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        return Some((metadata.dev(), metadata.ino()));
    }
    #[cfg(windows)]
    {
        let _ = metadata;
        let (volume, index, _) = windows_local_file_info(_path)?;
        return Some((volume, index));
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (_path, metadata);
        None
    }
}

/// Returns the host hard-link count when the platform exposes it.
#[cfg(any(windows, test))]
fn local_file_link_count(path: &std::path::Path) -> Option<u64> {
    let metadata = std::fs::metadata(path).ok()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        return Some(metadata.nlink());
    }
    #[cfg(windows)]
    {
        let _ = metadata;
        return windows_local_file_info(path).map(|(_, _, links)| links);
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = metadata;
        None
    }
}

/// Canonicalizes an existing path and applies Windows case-insensitive folding.
#[cfg(any(windows, test))]
fn normalize_local_file_mode_path(path: &std::path::Path) -> Option<String> {
    let absolute = std::fs::canonicalize(path).or_else(|_| {
        if path.is_absolute() {
            Ok(path.to_path_buf())
        } else {
            std::env::current_dir().map(|cwd| cwd.join(path))
        }
    });
    let normalized = absolute.ok()?.to_string_lossy().into_owned();
    #[cfg(windows)]
    return Some(normalized.to_lowercase());
    #[cfg(not(windows))]
    return Some(normalized);
}

/// Reads stable Windows volume, file-index, and hard-link-count fields.
#[cfg(windows)]
fn windows_local_file_info(path: &std::path::Path) -> Option<(u64, u64, u64)> {
    use std::ffi::c_void;
    use std::os::windows::io::AsRawHandle;

    #[repr(C)]
    struct FileTime {
        low: u32,
        high: u32,
    }

    #[repr(C)]
    struct ByHandleFileInformation {
        attributes: u32,
        creation_time: FileTime,
        last_access_time: FileTime,
        last_write_time: FileTime,
        volume_serial: u32,
        file_size_high: u32,
        file_size_low: u32,
        number_of_links: u32,
        file_index_high: u32,
        file_index_low: u32,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        /// Reads stable filesystem identity for one open Windows handle.
        fn GetFileInformationByHandle(
            file: *mut c_void,
            information: *mut ByHandleFileInformation,
        ) -> i32;
    }

    let file = std::fs::File::open(path).ok()?;
    let mut information = std::mem::MaybeUninit::<ByHandleFileInformation>::uninit();
    let status = unsafe {
        GetFileInformationByHandle(file.as_raw_handle(), information.as_mut_ptr())
    };
    if status == 0 {
        return None;
    }
    let information = unsafe { information.assume_init() };
    Some((
        u64::from(information.volume_serial),
        (u64::from(information.file_index_high) << 32)
            | u64::from(information.file_index_low),
        u64::from(information.number_of_links),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_FIXTURE_ID: AtomicU64 = AtomicU64::new(0);

    /// Creates an isolated filesystem fixture directory for one mode-state test.
    fn mode_test_dir(label: &str) -> PathBuf {
        let id = NEXT_FIXTURE_ID.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "elephc_magician_mode_{label}_{}_{id}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir(&path).expect("create mode test directory");
        path
    }

    /// Verifies canonical and dot-segment aliases resolve to the same mode identity.
    #[test]
    fn local_file_mode_resolves_path_aliases() {
        let dir = mode_test_dir("aliases");
        let file = dir.join("sample.txt");
        std::fs::write(&file, b"data").expect("create mode test file");
        let alias = dir.join(".").join("sample.txt");
        let canonical = std::fs::canonicalize(&file).expect("canonicalize mode test file");
        let mut context = ElephcEvalContext::new();

        context.remember_local_file_mode(&alias, 0o640);

        assert_eq!(context.local_file_mode(&file), Some(0o640));
        assert_eq!(context.local_file_mode(&canonical), Some(0o640));
        std::fs::remove_dir_all(dir).expect("remove mode test directory");
    }

    /// Verifies rename moves mode state and copy replaces destination mode state.
    #[test]
    fn local_file_mode_follows_rename_and_copy() {
        let dir = mode_test_dir("rename_copy");
        let source = dir.join("source.txt");
        let renamed = dir.join("renamed.txt");
        let copied = dir.join("copied.txt");
        std::fs::write(&source, b"source").expect("create source file");
        std::fs::write(&copied, b"old destination").expect("create copy destination");
        let mut context = ElephcEvalContext::new();
        context.remember_local_file_mode(&source, 0o600);
        context.remember_local_file_mode(&copied, 0o777);

        let source_mode = context.capture_local_file_mode(&source);
        let renamed_mode = context.capture_local_file_mode(&renamed);
        std::fs::rename(&source, &renamed).expect("rename source file");
        context.rename_local_file_mode(source_mode, renamed_mode, &renamed);
        assert_eq!(context.local_file_mode(&source), None);
        assert_eq!(context.local_file_mode(&renamed), Some(0o600));

        let source_mode = context.capture_local_file_mode(&renamed);
        let destination_mode = context.capture_local_file_mode(&copied);
        std::fs::copy(&renamed, &copied).expect("copy renamed file");
        context.copy_local_file_mode(source_mode.as_ref(), destination_mode.as_ref(), &copied);
        assert_eq!(context.local_file_mode(&renamed), Some(0o600));
        assert_eq!(context.local_file_mode(&copied), Some(0o600));
        std::fs::remove_dir_all(dir).expect("remove mode test directory");
    }

    /// Verifies hard-link aliases retain mode state until the last link is removed.
    #[test]
    fn local_file_mode_tracks_hard_links_and_recreation() {
        let dir = mode_test_dir("hard_link");
        let source = dir.join("source.txt");
        let alias = dir.join("alias.txt");
        std::fs::write(&source, b"source").expect("create hard-link source");
        let mut context = ElephcEvalContext::new();
        context.remember_local_file_mode(&source, 0o620);

        let source_mode = context.capture_local_file_mode(&source);
        std::fs::hard_link(&source, &alias).expect("create hard-link alias");
        context.copy_local_file_mode(source_mode.as_ref(), None, &alias);
        assert_eq!(context.local_file_mode(&alias), Some(0o620));

        let removed_source = context.capture_local_file_mode(&source);
        std::fs::remove_file(&source).expect("remove original hard link");
        context.unlink_local_file_mode(removed_source);
        assert_eq!(context.local_file_mode(&alias), Some(0o620));

        let removed_alias = context.capture_local_file_mode(&alias);
        std::fs::remove_file(&alias).expect("remove final hard link");
        context.unlink_local_file_mode(removed_alias);
        std::fs::write(&alias, b"replacement").expect("recreate alias path");
        assert_eq!(context.local_file_mode(&alias), None);
        std::fs::remove_dir_all(dir).expect("remove mode test directory");
    }

    /// Verifies Windows case-insensitive path aliases share one emulated mode.
    #[cfg(windows)]
    #[test]
    fn local_file_mode_resolves_windows_case_aliases() {
        let dir = mode_test_dir("case_alias");
        let file = dir.join("CaseSample.txt");
        std::fs::write(&file, b"data").expect("create Windows case test file");
        let case_alias = dir.join("casesample.TXT");
        let mut context = ElephcEvalContext::new();

        context.remember_local_file_mode(&file, 0o604);

        assert_eq!(context.local_file_mode(&case_alias), Some(0o604));
        std::fs::remove_dir_all(dir).expect("remove mode test directory");
    }
}
