use std::fs;

use tempfile::TempDir;
use uuid::Uuid;

use crate::ids::FfiMediaUuid;
use crate::library::{FfiLibrary, ffi_create_library};
use lasco_core::identifiers::RemoteUuid;
use lasco_core::storage::StorageMockMemoryFaulty;

pub(super) struct Device {
    app_dir: TempDir,
    source_dir: TempDir,
    pub(super) library: std::sync::Arc<FfiLibrary>,
}

impl Device {
    pub(super) fn new() -> Self {
        let app_dir = TempDir::new().unwrap();
        let source_dir = TempDir::new().unwrap();
        let app_path = app_dir.path().to_string_lossy().into_owned();
        ffi_create_library(
            "ffi-test".to_string(),
            "alice".to_string(),
            "secret".to_string(),
            Some(app_path.clone()),
        )
        .unwrap();
        let library = FfiLibrary::open(
            Some("ffi-test".to_string()),
            "alice".to_string(),
            "secret".to_string(),
            Some(app_path),
        )
        .unwrap();

        Self {
            app_dir,
            source_dir,
            library,
        }
    }

    /// Creates a new local client with the same library metadata but no local operations.
    /// Subsequent state must therefore be obtained by fetching its configured remote.
    pub(super) fn join_existing(source: &Self) -> Self {
        let app_dir = TempDir::new().unwrap();
        let source_dir = TempDir::new().unwrap();
        let library_id = source.library.library_id().value;
        let source_root = source.app_dir.path();
        let target_root = app_dir.path();
        copy_file(
            &source_root.join("config.json"),
            &target_root.join("config.json"),
        );
        let source_library = source_root.join("libraries").join(&library_id);
        let target_library = target_root.join("libraries").join(&library_id);
        copy_file(
            &source_library.join("library.json"),
            &target_library.join("library.json"),
        );
        copy_dir(
            &source_library.join("local_state").join("library"),
            &target_library.join("local_state").join("library"),
        );

        let library = FfiLibrary::open(
            Some("ffi-test".to_string()),
            "alice".to_string(),
            "secret".to_string(),
            Some(target_root.to_string_lossy().into_owned()),
        )
        .unwrap();

        Self {
            app_dir,
            source_dir,
            library,
        }
    }

    pub(super) fn add_remote(
        &self,
        storage: &StorageMockMemoryFaulty,
    ) -> crate::ids::FfiRemoteUuid {
        self.add_named_remote("test-remote", storage)
    }

    pub(super) fn add_named_remote(
        &self,
        name: &str,
        storage: &StorageMockMemoryFaulty,
    ) -> crate::ids::FfiRemoteUuid {
        let remote_dir = self.source_dir.path().join("configured-remote");
        let remote_id = self
            .library
            .add_remote_fixed_path(name.to_string(), remote_dir.to_string_lossy().into_owned())
            .unwrap();
        let core_id: RemoteUuid = remote_id.clone().try_into().unwrap();
        self.library.register_test_remote(core_id, storage.clone());
        self.library
            .initialize_remote(remote_id.clone(), None)
            .unwrap();
        remote_id
    }

    pub(super) fn register_existing_remote(
        &self,
        remote_id: &crate::ids::FfiRemoteUuid,
        storage: &StorageMockMemoryFaulty,
    ) {
        let core_id: RemoteUuid = remote_id.clone().try_into().unwrap();
        self.library.register_test_remote(core_id, storage.clone());
    }

    pub(super) fn import_uuid_media(&mut self) -> (FfiMediaUuid, Vec<u8>) {
        let fixture_id = Uuid::new_v4();
        let expected = fixture_id.as_u128().to_le_bytes().to_vec();
        let source = self.source_dir.path().join(format!("{fixture_id}.bin"));
        fs::write(&source, &expected).unwrap();
        let result = self
            .library
            .import_media(
                source.to_string_lossy().into_owned(),
                None,
                Some(format!("{fixture_id}.bin")),
                None,
                None,
            )
            .unwrap();
        (result.media_id, expected)
    }

    pub(super) fn import_uuid_media_batch(&mut self, count: usize) -> Vec<(FfiMediaUuid, Vec<u8>)> {
        (0..count).map(|_| self.import_uuid_media()).collect()
    }
}

fn copy_file(source: &std::path::Path, target: &std::path::Path) {
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::copy(source, target).unwrap();
}

fn copy_dir(source: &std::path::Path, target: &std::path::Path) {
    fs::create_dir_all(target).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let target_path = target.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target_path);
        } else {
            copy_file(&entry.path(), &target_path);
        }
    }
}
