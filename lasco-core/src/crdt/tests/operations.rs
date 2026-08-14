use chrono::Utc;
use uuid::Uuid;

use crate::crdt::{CrdtOperation, CrdtState, DeviceId, Dot, OperationContent};
use crate::identifiers::{AlbumUuid, GroupUuid, MediaUuid};
use crate::operations::LibraryUsername;

pub(super) fn album(n: u128) -> AlbumUuid {
    AlbumUuid::from_uuid(Uuid::from_u128(n))
}

pub(super) fn media(n: u128) -> MediaUuid {
    MediaUuid::from_uuid(Uuid::from_u128(n))
}

pub(super) fn group(n: u128) -> GroupUuid {
    GroupUuid::from_uuid(Uuid::from_u128(n))
}

pub(super) fn operation(dot: Dot, content: OperationContent) -> CrdtOperation {
    CrdtOperation {
        dot,
        author: LibraryUsername("test".into()),
        timestamp: Utc::now(),
        content,
    }
}

/// Applies every possible delivery order for a small, logically valid operation history.
pub(super) fn assert_every_delivery_order(
    operations: &[CrdtOperation],
    mut assert_result: impl FnMut(&CrdtState),
) {
    let mut expected = CrdtState::new(DeviceId(99));
    expected.merge_all(operations.iter());
    let mut order: Vec<_> = (0..operations.len()).collect();
    visit_delivery_orders(operations, &expected, &mut order, 0, &mut assert_result);
}

fn visit_delivery_orders(
    operations: &[CrdtOperation],
    expected: &CrdtState,
    order: &mut [usize],
    first_unfixed: usize,
    assert_result: &mut impl FnMut(&CrdtState),
) {
    if first_unfixed == order.len() {
        let mut state = CrdtState::new(DeviceId(99));
        state.merge_all(order.iter().map(|index| &operations[*index]));
        assert_eq!(
            state, *expected,
            "delivery order {order:?} did not converge to the canonical CRDT state"
        );
        assert_result(&state);
        return;
    }
    for index in first_unfixed..order.len() {
        order.swap(first_unfixed, index);
        visit_delivery_orders(
            operations,
            expected,
            order,
            first_unfixed + 1,
            assert_result,
        );
        order.swap(first_unfixed, index);
    }
}
