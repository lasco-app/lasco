use std::collections::{BTreeMap, BTreeSet};

use crate::crdt::{CrdtOperation, CrdtState, DeviceId, Dot};

use super::simulator::Replica;

fn canonical(operations: &BTreeMap<Dot, CrdtOperation>, device_id: DeviceId) -> CrdtState {
    let mut state = CrdtState::new(device_id);
    state.merge_all(operations.values());
    state
}

pub(super) fn assert_replica_matches_known(replica: &Replica) {
    assert_eq!(replica.state, canonical(&replica.known, replica.device_id));
}

pub(super) fn assert_replica_matches_global(
    replica: &Replica,
    covered: &BTreeSet<Dot>,
    operations: &[CrdtOperation],
) {
    let unique_operations: BTreeMap<_, _> = operations
        .iter()
        .filter(|operation| covered.contains(&operation.dot))
        .map(|operation| (operation.dot, operation.clone()))
        .collect();
    let expected = canonical(&unique_operations, DeviceId(0));
    let mut actual = replica.state.clone();
    actual.device_id = DeviceId(0);
    assert_eq!(actual, expected);
}
