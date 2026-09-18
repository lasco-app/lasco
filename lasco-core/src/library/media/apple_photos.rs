//! Apple Photos / iCloud Photos import provenance.

use chrono::{DateTime, Utc};

use crate::{AlbumUuid, MediaUuid};
use crate::crdt::{ApplePhotosCollectionKind, ApplePhotosCollectionLink, ApplePhotosResourceOrigin, ApplePhotosResourceType, OperationContent};
use crate::error::LibraryError;
use crate::library::Library;
use crate::operations::{ApplePhotosCloudAssetId, ApplePhotosCloudCollectionId};

/// Adds one immutable association between a Lasco media item and an Apple Photos resource.
///
/// Callers emit one association for each resource after the resource has either been added or
/// reused by content hash. A repeated call is harmless: the operation log remains append-only and
/// the revision lookup below only cares that every selected resource has an association.
pub fn record_resource_origin(
    library: &Library,
    origin: ApplePhotosResourceOrigin,
) -> Result<(), LibraryError> {
    library.record_local_operation(
        Utc::now(),
        OperationContent::ApplePhotosResourceOriginAdded(origin),
    )
}

/// Records immutable provenance for an imported Apple Photos folder or album.
pub fn record_collection_link(library: &Library, link: ApplePhotosCollectionLink) -> Result<(), LibraryError> {
    library.record_local_operation(Utc::now(), OperationContent::ApplePhotosCollectionLinkAdded(link))
}

/// Returns the canonical Lasco album for an Apple collection. The earliest CRDT dot wins if two
/// clients concurrently linked one collection to distinct albums.
pub fn collection_album_id(
    library: &Library,
    cloud_collection_id: &ApplePhotosCloudCollectionId,
    kind: ApplePhotosCollectionKind,
) -> Option<AlbumUuid> {
    library
        .inner
        .state
        .read()
        .apple_photos_collection_links
        .iter()
        .filter(|entry| entry.link.cloud_collection_id == *cloud_collection_id && entry.link.kind == kind)
        .min_by_key(|entry| entry.dot)
        .map(|entry| entry.link.album_id)
}

/// Returns the media associated with an exact Apple Photos asset revision, in the caller's
/// selected-resource order. The manifest is compared directly; it is not persisted as a hash.
///
/// Repeated origin operations for an already-imported resource collapse to one descriptor here;
/// operations are append-only, while this query is about the current revision's resource set.
pub fn complete_revision_media_ids(
    library: &Library,
    cloud_asset_id: &ApplePhotosCloudAssetId,
    modification_date: Option<DateTime<Utc>>,
    expected_resources: &[(ApplePhotosResourceType, String)],
) -> Option<Vec<MediaUuid>> {
    let state = library.inner.state.read();
    let mut matching = std::collections::BTreeMap::new();
    for entry in state.apple_photos_resource_origins.iter().filter(|entry| {
        let origin = &entry.origin;
        origin.cloud_asset_id == *cloud_asset_id && origin.modification_date == modification_date
    }) {
        matching
            .entry((entry.origin.resource_type, entry.origin.filename.clone()))
            .or_insert(entry.origin.media_id);
    }
    let expected_in_caller_order = expected_resources.to_vec();
    let mut expected = expected_in_caller_order.clone();
    expected.sort();
    expected.dedup();
    if matching.keys().eq(expected.iter()) {
        expected_in_caller_order
            .into_iter()
            .map(|descriptor| matching.get(&descriptor).copied())
            .collect()
    } else {
        None
    }
}
