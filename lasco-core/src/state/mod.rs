mod types;
mod views;

pub use types::{
    AlbumBrowseItem, AlbumEntry, ComputedViews, GroupEntry, MediaEntry, ReconstructedState,
};
pub use views::build_computed_views;

#[derive(Clone, Debug, Default)]
pub struct OperationState {
    pub reconstructed: ReconstructedState,
    pub views: ComputedViews,
}

impl OperationState {
    #[must_use]
    pub fn from_reconstructed(reconstructed: ReconstructedState) -> Self {
        let views = build_computed_views(&reconstructed);
        Self {
            reconstructed,
            views,
        }
    }
}
