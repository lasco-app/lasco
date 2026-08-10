use lasco_core::identifiers::{AlbumUuid, GroupUuid, LibraryId, MediaUuid, RemoteUuid};

use crate::error::LascoError;

macro_rules! ffi_id {
    ($(#[$meta:meta])* $ffi:ident, $core:ty, $label:literal) => {
        $(#[$meta])*
        #[derive(uniffi::Record, Debug, Clone, PartialEq, Eq, Hash)]
        pub struct $ffi {
            pub value: String,
        }

        impl From<$core> for $ffi {
            fn from(value: $core) -> Self {
                Self {
                    value: value.to_string(),
                }
            }
        }

        impl TryFrom<$ffi> for $core {
            type Error = LascoError;

            fn try_from(value: $ffi) -> Result<Self, Self::Error> {
                uuid::Uuid::parse_str(&value.value)
                    .map(<$core>::from_uuid)
                    .map_err(|e| LascoError::Other {
                        msg: format!("invalid {}: {e}", $label),
                    })
            }
        }
    };
}

ffi_id!(
    /// A media identifier exposed to UniFFI as a record so Swift and Kotlin receive
    /// a distinct type. Do not replace this with `uniffi::custom_type!` backed by
    /// `String`: UniFFI generates custom string types as `String` aliases, allowing
    /// media IDs to be accidentally passed where another ID kind is required.
    FfiMediaUuid,
    MediaUuid,
    "media id"
);
ffi_id!(FfiAlbumUuid, AlbumUuid, "album id");
ffi_id!(FfiGroupUuid, GroupUuid, "group id");
ffi_id!(FfiRemoteUuid, RemoteUuid, "remote id");
ffi_id!(FfiLibraryId, LibraryId, "library id");
