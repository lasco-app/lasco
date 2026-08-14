mod types;
mod views;

pub use types::{AlbumBrowseItem, ComputedViews};
pub use views::build_computed_views;

#[derive(Clone, Debug, Default)]
pub struct InMemoryLibraryState {
    pub(crate) crdt: crate::crdt::CrdtState,
    pub views: ComputedViews,
}

impl InMemoryLibraryState {
    #[must_use]
    pub fn new(crdt: crate::crdt::CrdtState) -> Self {
        let mut state = Self {
            crdt,
            views: ComputedViews::default(),
        };
        state.rebuild_views();
        state
    }
    pub fn rebuild_views(&mut self) {
        self.views = build_computed_views(&self.crdt);
    }
    pub fn apply(&mut self, operation: &crate::crdt::CrdtOperation) {
        self.crdt.apply(operation);
        self.rebuild_views();
    }
}
