use crate::operations::OperationGroup;

mod types;
mod reconstruct;
mod views;
#[cfg(test)]
mod tests;

pub use types::{AlbumEntry, ComputedViews, GroupEntry, MediaEntry, ReconstructedState};
pub use reconstruct::reconstruct_state;
pub use views::build_computed_views;

#[derive(Clone, Debug, Default)]
pub struct OperationState {
    pub reconstructed: ReconstructedState,
    pub views: ComputedViews,
}

impl OperationState {
    pub fn build(sorted_ops: &[OperationGroup]) -> Self {
        let reconstructed = reconstruct_state(sorted_ops);
        let views = build_computed_views(&reconstructed);
        OperationState { reconstructed, views }
    }
}
