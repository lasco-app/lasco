//! Docker-backed FFI smoke test. Run explicitly with:
//! `cargo test -p lasco-ffi ffi_smb_remote -- --ignored`

use crate::library::ffi_test_smb_remote;
use super::utils::Device;

#[test]
fn ffi_smb_remote_persists_connection_metadata_but_not_password() {
    let device = Device::new();
    let remote_id = device
        .library
        .add_remote_smb(
            "NAS".to_string(),
            "nas.local".to_string(),
            445,
            "photos".to_string(),
            "lasco".to_string(),
            "alice".to_string(),
            "not-exposed".to_string(),
            Some("WORKGROUP".to_string()),
        )
        .expect("save SMB remote");
    let remote = device
        .library
        .list_remotes()
        .into_iter()
        .find(|remote| remote.remote_id == remote_id)
        .expect("saved remote");
    assert_eq!(remote.kind, "smb");
    assert_eq!(remote.server.as_deref(), Some("nas.local"));
    assert_eq!(remote.port, Some(445));
    assert_eq!(remote.share.as_deref(), Some("photos"));
    assert_eq!(remote.username.as_deref(), Some("alice"));
    assert_eq!(remote.domain.as_deref(), Some("WORKGROUP"));
}

/// Exercises the public FFI boundary against the Samba server supplied by
/// `smb2`, including the write/read/delete probe used by mobile clients.
#[test]
#[ignore = "requires Docker to start smb2's Samba test server"]
fn ffi_smb_remote_connection_probe() {
    let runtime = tokio::runtime::Runtime::new().expect("Tokio runtime");
    let servers = runtime
        .block_on(smb2::testing::TestServers::start())
        .expect("start Samba test server");
    ffi_test_smb_remote(
        "127.0.0.1".to_string(),
        smb2::testing::auth_port(),
        "private".to_string(),
        "lasco-ffi-test".to_string(),
        "testuser".to_string(),
        "testpass".to_string(),
        None,
    )
    .expect("FFI SMB connection probe");
    drop(servers);
}
