//! Apple Photos / iCloud Photos import provenance.

use chrono::{DateTime, Utc};

use crate::crdt::{ApplePhotosResourceOrigin, ApplePhotosResourceType, OperationContent};
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

/// Returns true when the library has recorded exactly the selected resources for this iCloud
/// Photos asset revision. The manifest is compared directly; it is not persisted as a hash.
pub fn has_complete_revision(
    library: &Library,
    cloud_asset_id: &ApplePhotosCloudAssetId,
    modification_date: Option<DateTime<Utc>>,
    expected_resources: &[(ApplePhotosResourceType, String)],
) -> bool {
    let state = library.inner.state.read();
    let mut matching: Vec<_> = state
        .apple_photos_resource_origins
        .iter()
        .filter(|entry| {
            let origin = &entry.origin;
            origin.cloud_asset_id == *cloud_asset_id
                && origin.modification_date == modification_date
        })
        .map(|entry| (entry.origin.resource_type, entry.origin.filename.clone()))
        .collect();
    matching.sort();
    matching.dedup();
    let mut expected = expected_resources.to_vec();
    expected.sort();
    expected.dedup();
    matching == expected
}
