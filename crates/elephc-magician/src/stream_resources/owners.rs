//! Purpose:
//! Detaches every PHP runtime cell owned by eval-local resource records.
//!
//! Called from:
//! - Eval context teardown before release callbacks can re-enter Magician.
//!
//! Key details:
//! - Native hash and cURL handles remain in their tables for ordinary `Drop` cleanup.
//! - PHP owner records disappear as one batch before any destructor can execute.

use super::*;

impl EvalStreamResources {
    /// Reports whether any resource record still publishes a PHP runtime-cell owner.
    pub(crate) fn has_php_cell_owners(&self) -> bool {
        self.stream_contexts
            .values()
            .any(|context| !context.options.is_empty())
            || self
                .user_wrapper_streams
                .values()
                .any(|stream| !stream.object.is_empty())
            || self
                .user_wrapper_directories
                .values()
                .any(|directory| !directory.object.is_empty())
            || {
                #[cfg(feature = "curl")]
                {
                    self.curl_easy_handles.values().any(|handle| {
                        !handle.private_value.is_empty()
                            || handle.callback_owners.iter().any(|owner| !owner.is_empty())
                    })
                }
                #[cfg(not(feature = "curl"))]
                {
                    false
                }
            }
    }

    /// Detaches every resource-owned PHP cell while leaving native handles ready for `Drop`.
    pub(crate) fn take_php_cell_owners(
        &mut self,
    ) -> elephc_memoir::runtime_owners::RuntimeOwnerBatch {
        self.default_stream_context = None;
        let mut owners = elephc_memoir::runtime_owners::RuntimeOwnerBatch::new(
            self.stream_contexts
                .drain()
                .filter_map(|(_, context)| context.options.take_optional()),
        );
        owners.extend(
            self.user_wrapper_streams
                .drain()
                .filter_map(|(_, stream)| stream.object.take_optional()),
        );
        owners.extend(
            self.user_wrapper_directories
                .drain()
                .filter_map(|(_, directory)| directory.object.take_optional()),
        );
        #[cfg(feature = "curl")]
        owners.append(self.take_curl_easy_php_owners());
        owners
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Teardown removes every resource record that can publish a PHP owner.
    #[test]
    fn resource_owner_batch_detaches_stream_contexts_and_wrappers() {
        let mut resources = EvalStreamResources::default();
        let options = RuntimeCellHandle::from_raw(8usize as *mut _);
        let stream = RuntimeCellHandle::from_raw(16usize as *mut _);
        let directory = RuntimeCellHandle::from_raw(24usize as *mut _);
        resources.open_stream_context(Some(options));
        resources.open_user_wrapper_stream(stream, "Wrapper", "test://stream", "r");
        resources.open_user_wrapper_directory(directory, "Wrapper");

        assert!(resources.has_php_cell_owners());

        let mut owners = resources.take_php_cell_owners().iter().collect::<Vec<_>>();
        owners.sort_by_key(|owner| owner.as_ptr() as usize);
        assert_eq!(owners, vec![options, stream, directory]);
        assert!(resources.stream_contexts.is_empty());
        assert!(resources.user_wrapper_streams.is_empty());
        assert!(resources.user_wrapper_directories.is_empty());
        assert!(!resources.has_php_cell_owners());
    }
}
