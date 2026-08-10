use crate::operations::OperationGroup;

mod reconstruct;
#[cfg(test)]
mod tests;
mod types;
mod views;

pub use reconstruct::reconstruct_state;
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
    pub fn from_reconstructed(reconstructed: ReconstructedState) -> Self {
        let views = build_computed_views(&reconstructed);
        Self {
            reconstructed,
            views,
        }
    }

    pub fn build(sorted_ops: &[OperationGroup]) -> Self {
        let reconstructed = reconstruct_state(sorted_ops);
        Self::from_reconstructed(reconstructed)
    }
}
