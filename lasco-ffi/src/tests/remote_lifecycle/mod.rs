use lasco_core::storage::StorageMockMemoryFaulty;

use super::utils;

/// The directory holding everything this client caches about one remote.
fn remote_dir(device: &utils::Device, remote_id: &crate::ids::FfiRemoteUuid) -> std::path::PathBuf {
    device
        .app_dir
        .path()
        .join("libraries")
        .join(device.library.library_id().value)
        .join("remotes")
        .join(&remote_id.value)
}

#[test]
fn removing_a_remote_deletes_what_this_client_cached_about_it() {
    let mut device = utils::Device::new();
    let remote = StorageMockMemoryFaulty::new();
    let remote_id = device.add_remote(&remote);
    device.import_uuid_media();
    device.library.push_remote(remote_id.clone(), None).unwrap();

    let dir = remote_dir(&device, &remote_id);
    assert!(dir.exists(), "a push must leave state behind to delete");

    device.library.remove_remote(remote_id.clone()).unwrap();

    assert!(!dir.exists());
    assert!(
        device
            .library
            .list_remotes()
            .iter()
            .all(|remote| remote.remote_id != remote_id)
    );
}
