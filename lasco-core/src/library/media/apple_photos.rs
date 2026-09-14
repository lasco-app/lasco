//! Apple Photos / iCloud Photos import provenance.

use chrono::{DateTime, Utc};

use crate::crdt::{ApplePhotosResourceOrigin, OperationContent};
use crate::operations::ApplePhotosCloudAssetId;
use crate::error::LibraryError;
use crate::library::Library;

/// Adds one immutable association between a Lasco media item and an Apple Photos resource.
///
/// Callers emit one association for each resource after the resource has either been added or
/// reused by content hash. A repeated call is harmless: the operation log remains append-only and
/// the revision lookup below only cares that every selected resource has an association.
pub fn record_resource_origin(
    library: &Library,
    origin: ApplePhotosResourceOrigin,
) -> Result<(), LibraryError> {
    library.record_local_operation(Utc::now(), OperationContent::ApplePhotosResourceOriginAdded(origin))
}

/// Returns true when the library has recorded a complete selected-resource manifest for this
/// exact iCloud Photos asset revision.
pub fn has_complete_revision(
    library: &Library,
    cloud_asset_id: &ApplePhotosCloudAssetId,
    modification_date: Option<DateTime<Utc>>,
    manifest_hash: &str,
    resource_count: u32,
) -> bool {
    let state = library.inner.state.read();
    let matching = state
        .apple_photos_resource_origins
        .iter()
        .filter(|entry| {
            let origin = &entry.origin;
            origin.cloud_asset_id == *cloud_asset_id
                && origin.modification_date == modification_date
                && origin.manifest_hash == manifest_hash
                && origin.resource_count == resource_count
        })
        .count();
    matching >= usize::try_from(resource_count).unwrap_or(usize::MAX)
}
