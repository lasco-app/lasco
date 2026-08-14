#![allow(
    clippy::unused_async,
    reason = "Public library APIs remain async for FFI and caller compatibility even when a local mutation is synchronous."
)]

mod atomic_file;
pub mod client;
pub mod config_json;
pub mod crdt;
pub mod encryption;
pub mod error;
pub mod identifiers;
pub mod library;
pub mod library_json;
pub mod operations;
pub mod remote;
pub mod s3_secret;
pub mod session;
pub mod state;
pub mod storage;

pub const LIBRARY_DIR: &str = "library";

pub use crdt::GroupEntry;
pub use encryption::blob_key::BlobKey;
pub use encryption::kek::{Kek, LibrarySalt};
pub use encryption::master_key::MasterKey;
pub use identifiers::{AlbumUuid, GroupUuid, LibraryId, MediaUuid, UserUuid};
pub use library::albums::AlbumSummary;
pub use library::media::MediaEntry;
