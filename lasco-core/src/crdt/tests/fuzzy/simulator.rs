use std::collections::{BTreeMap, BTreeSet};

use rand::{Rng as _, SeedableRng as _};

use crate::crdt::{CrdtOperation, CrdtState, DeviceId, Dot};

use super::{actions, assertions, sync, values::Values};

#[derive(Clone)]
pub(super) struct Replica {
    pub(super) device_id: DeviceId,
    pub(super) state: CrdtState,
    pub(super) known: BTreeMap<Dot, CrdtOperation>,
    pub(super) arrival_order: Vec<Dot>,
    pub(super) remote_cursor: usize,
}

impl Replica {
    fn new(device_id: DeviceId) -> Self {
        Self {
            device_id,
            state: CrdtState::new(device_id),
            known: BTreeMap::new(),
            arrival_order: Vec::new(),
            remote_cursor: 0,
        }
    }

    pub(super) fn receive(&mut self, operation: &CrdtOperation) {
        self.state.apply(operation);
        if self
            .known
            .insert(operation.dot, operation.clone())
            .is_none()
        {
            self.arrival_order.push(operation.dot);
        }
    }
}

#[derive(Default)]
pub(super) struct Remote {
    pub(super) operations: Vec<CrdtOperation>,
    pub(super) covered: BTreeSet<Dot>,
}

impl Remote {
    pub(super) fn receive(&mut self, operation: &CrdtOperation) {
        self.operations.push(operation.clone());
        self.covered.insert(operation.dot);
    }
}

pub(super) struct Simulator {
    pub(super) replicas: Vec<Replica>,
    remote: Remote,
    rng: rand::rngs::StdRng,
    values: Values,
    weights: actions::OperationWeights,
}

impl Simulator {
    pub(super) fn from_seed(seed: u64) -> Self {
        let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
        let device_count: u128 = rng.gen_range(2..=5);
        let replicas = (1..=device_count)
            .map(|id| Replica::new(DeviceId(u128::from(id))))
            .collect();
        let weights = actions::OperationWeights::from_rng(&mut rng);
        Self {
            replicas,
            remote: Remote::default(),
            rng,
            values: Values::new(seed),
            weights,
        }
    }

    pub(super) fn run(&mut self) {
        let rounds = self.rng.gen_range(24..=48);
        for _ in 0..rounds {
            self.run_round();
        }
        self.converge();
    }

    fn run_round(&mut self) {
        if self.rng.gen_bool(0.10) {
            self.converge();
        } else {
            self.run_action_round();
        }
    }

    fn run_action_round(&mut self) {
        let actor = self.rng.gen_range(0..self.replicas.len());
        if self.rng.gen_bool(0.35) {
            self.fetch(actor);
        }
        let action_count = self.rng.gen_range(1..=10);
        for _ in 0..action_count {
            let operation = actions::draw_operation(
                &mut self.replicas[actor],
                &mut self.values,
                &self.weights,
                &mut self.rng,
            );
            self.replicas[actor].receive(&operation);
            assertions::assert_replica_matches_known(&self.replicas[actor]);
        }
        if self.rng.gen_bool(0.40) {
            self.push(actor);
        }
    }

    fn fetch(&mut self, target: usize) {
        sync::fetch(&self.remote, &mut self.replicas[target], &mut self.rng);
        assertions::assert_replica_matches_known(&self.replicas[target]);
    }

    fn push(&mut self, source: usize) {
        let source = self.replicas[source].clone();
        sync::push(&source, &mut self.remote, &mut self.rng);
    }

    fn converge(&mut self) {
        for replica in &self.replicas {
            sync::push(replica, &mut self.remote, &mut self.rng);
        }
        for replica in &mut self.replicas {
            sync::fetch(&self.remote, replica, &mut self.rng);
            assertions::assert_replica_matches_known(replica);
            assertions::assert_replica_matches_global(
                replica,
                &self.remote.covered,
                &self.remote.operations,
            );
        }
    }
}
