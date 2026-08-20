use rand::Rng as _;

use super::simulator::{Remote, Replica};

/// Fetches every remote-log entry after the device's previous fetch.
pub(super) fn fetch(remote: &Remote, target: &mut Replica, rng: &mut rand::rngs::StdRng) {
    let batch = &remote.operations[target.remote_cursor..];
    target.remote_cursor = remote.operations.len();
    deliver_to_replica(batch, target, rng);
}

/// Pushes every operation the remote does not yet have in one batch.
pub(super) fn push(source: &Replica, remote: &mut Remote, rng: &mut rand::rngs::StdRng) {
    let missing: Vec<_> = source
        .arrival_order
        .iter()
        .filter(|dot| !remote.covered.contains(dot))
        .filter_map(|dot| source.known.get(dot))
        .cloned()
        .collect();
    deliver_to_remote(&missing, remote, rng);
}

fn deliver_to_replica(
    batch: &[crate::crdt::CrdtOperation],
    target: &mut Replica,
    rng: &mut rand::rngs::StdRng,
) {
    for operation in batch {
        target.receive(operation);
    }
    if rng.gen_bool(0.10) {
        for operation in batch {
            target.receive(operation);
        }
    }
}

fn deliver_to_remote(
    batch: &[crate::crdt::CrdtOperation],
    remote: &mut Remote,
    rng: &mut rand::rngs::StdRng,
) {
    for operation in batch {
        remote.receive(operation);
    }
    if rng.gen_bool(0.10) {
        for operation in batch {
            remote.receive(operation);
        }
    }
}
