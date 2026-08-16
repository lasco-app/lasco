use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use rand::{Rng as _, SeedableRng as _};

use crate::identifiers::{AlbumUuid, GroupUuid, MediaUuid};

pub(super) struct Values {
    seed: u64,
    entity_rng: rand::rngs::StdRng,
    entity_ids: BTreeSet<u128>,
    next_write: u64,
}

impl Values {
    pub(super) fn new(seed: u64) -> Self {
        Self {
            seed,
            entity_rng: rand::rngs::StdRng::seed_from_u64(seed),
            entity_ids: BTreeSet::new(),
            next_write: 1,
        }
    }

    pub(super) fn album(&mut self) -> AlbumUuid {
        AlbumUuid::from_uuid(uuid::Uuid::from_u128(self.next_entity()))
    }

    pub(super) fn media(&mut self) -> MediaUuid {
        MediaUuid::from_uuid(uuid::Uuid::from_u128(self.next_entity()))
    }

    pub(super) fn group(&mut self) -> GroupUuid {
        GroupUuid::from_uuid(uuid::Uuid::from_u128(self.next_entity()))
    }

    pub(super) fn value(&mut self, kind: &str, device: u128) -> String {
        let write = self.next_write;
        self.next_write += 1;
        format!("seed-{}-{kind}-write-{write}-device-{device}", self.seed)
    }

    pub(super) fn timestamp(&mut self) -> DateTime<Utc> {
        let seconds = i64::try_from(self.next_write).unwrap();
        self.next_write += 1;
        DateTime::from_timestamp(seconds, 0).unwrap()
    }

    fn next_entity(&mut self) -> u128 {
        loop {
            let entity = self.entity_rng.r#gen();
            if self.entity_ids.insert(entity) {
                return entity;
            }
        }
    }
}
