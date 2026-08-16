use lasco_core::storage::{StorageMockMemoryFaulty, StorageMockOperation};
use proptest::prelude::*;
use rand::{Rng as _, SeedableRng as _};

use super::utils;

const MEDIA_COUNT: usize = 16;

#[test]
fn one_device_one_remote_preserves_media_bytes() {
    let mut device = utils::Device::new();
    let remote = StorageMockMemoryFaulty::new();
    let remote_id = device.add_remote(&remote);
    let media = device.import_uuid_media_batch(MEDIA_COUNT);

    device.library.push_remote(remote_id, None).unwrap();
    evict_media(&device, &media);
    assert_media_integrity(&device, &media);
}

#[test]
fn one_device_one_remote_recovers_first_media_upload_failure() {
    run_media_upload_failure(1);
}

#[test]
fn one_device_one_remote_recovers_middle_media_upload_failure() {
    run_media_upload_failure(8);
}

#[test]
fn one_device_one_remote_recovers_last_media_upload_failure() {
    run_media_upload_failure(16);
}

#[test]
fn one_device_one_remote_recovers_media_download_failure() {
    let mut device = utils::Device::new();
    let remote = StorageMockMemoryFaulty::new();
    let remote_id = device.add_remote(&remote);
    let media = device.import_uuid_media_batch(MEDIA_COUNT);

    device.library.push_remote(remote_id, None).unwrap();
    evict_media(&device, &media);
    remote.fail_next(StorageMockOperation::Get, "media/");
    assert!(
        device
            .library
            .get_media_bytes(media[0].0.clone(), None)
            .is_err()
    );
    assert_eq!(
        remote.pending_fault_count(),
        0,
        "the media read must hit the armed fault"
    );
    assert_media_integrity(&device, &media);
}

#[test]
fn one_device_two_remotes_relays_media_after_eviction() {
    let mut device = utils::Device::new();
    let first_remote = StorageMockMemoryFaulty::new();
    let second_remote = StorageMockMemoryFaulty::new();
    let first_remote_id = device.add_named_remote("first", &first_remote);
    let second_remote_id = device.add_named_remote("second", &second_remote);
    let media = device.import_uuid_media_batch(MEDIA_COUNT);

    device
        .library
        .push_remote(first_remote_id.clone(), None)
        .unwrap();
    evict_media(&device, &media);
    device
        .library
        .push_remote_from_remote(second_remote_id, first_remote_id, None)
        .unwrap();
    evict_media(&device, &media);
    first_remote.set_offline(true);
    assert_media_integrity(&device, &media);
}

#[test]
fn three_devices_one_remote_preserve_all_media_bytes() {
    let mut first = utils::Device::new();
    let remote = StorageMockMemoryFaulty::new();
    let remote_id = first.add_remote(&remote);
    let mut second = utils::Device::join_existing(&first);
    let mut third = utils::Device::join_existing(&first);
    second.register_existing_remote(&remote_id, &remote);
    third.register_existing_remote(&remote_id, &remote);

    let mut media = first.import_uuid_media_batch(MEDIA_COUNT);
    media.extend(second.import_uuid_media_batch(MEDIA_COUNT));
    media.extend(third.import_uuid_media_batch(MEDIA_COUNT));

    first.library.push_remote(remote_id.clone(), None).unwrap();
    second.library.push_remote(remote_id.clone(), None).unwrap();
    third.library.push_remote(remote_id.clone(), None).unwrap();

    for device in [&first, &second, &third] {
        device
            .library
            .fetch_remote(remote_id.clone(), None)
            .unwrap();
        evict_media(device, &media);
        assert_media_integrity(device, &media);
    }
}

fn run_media_upload_failure(match_number: usize) {
    let mut device = utils::Device::new();
    let remote = StorageMockMemoryFaulty::new();
    let remote_id = device.add_remote(&remote);
    let media = device.import_uuid_media_batch(MEDIA_COUNT);

    remote.fail_on_match(StorageMockOperation::PutAtomic, "media/", match_number);
    let error = device
        .library
        .push_remote(remote_id.clone(), None)
        .unwrap_err();
    assert!(
        error.to_string().contains("injected failure"),
        "push must report the armed storage failure, got: {error}"
    );

    device.library.push_remote(remote_id, None).unwrap();
    evict_media(&device, &media);
    assert_media_integrity(&device, &media);
}

fn evict_media(device: &utils::Device, _media: &[(crate::ids::FfiMediaUuid, Vec<u8>)]) {
    let known_ids = device.library.all_media_ids();
    device.library.evict_local_data(known_ids).unwrap();
}

fn assert_media_integrity(device: &utils::Device, media: &[(crate::ids::FfiMediaUuid, Vec<u8>)]) {
    for (id, expected) in media {
        assert_eq!(
            device.library.get_media_bytes(id.clone(), None).unwrap(),
            *expected
        );
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn three_devices_three_remotes_fuzzy(seed in any::<u64>()) {
        ThreeDeviceThreeRemoteTest::new(seed).run();
    }
}

struct ThreeDeviceThreeRemoteTest {
    devices: Vec<utils::Device>,
    remotes: Vec<StorageMockMemoryFaulty>,
    remote_ids: Vec<crate::ids::FfiRemoteUuid>,
    media: Vec<(crate::ids::FfiMediaUuid, Vec<u8>)>,
    rng: rand::rngs::StdRng,
}

impl ThreeDeviceThreeRemoteTest {
    fn new(seed: u64) -> Self {
        let first = utils::Device::new();
        let remotes = vec![
            StorageMockMemoryFaulty::new(),
            StorageMockMemoryFaulty::new(),
            StorageMockMemoryFaulty::new(),
        ];
        let remote_ids = remotes
            .iter()
            .enumerate()
            .map(|(index, remote)| first.add_named_remote(&format!("remote-{index}"), remote))
            .collect::<Vec<_>>();
        let second = utils::Device::join_existing(&first);
        let third = utils::Device::join_existing(&first);
        for (remote_id, remote) in remote_ids.iter().zip(&remotes) {
            second.register_existing_remote(remote_id, remote);
            third.register_existing_remote(remote_id, remote);
        }

        let mut devices = vec![first, second, third];
        let mut media = Vec::new();
        for device in &mut devices {
            media.push(device.import_uuid_media());
        }
        for device in &devices {
            for remote_id in &remote_ids {
                device.library.push_remote(remote_id.clone(), None).unwrap();
            }
        }
        for device in &devices {
            device
                .library
                .fetch_remote(remote_ids[0].clone(), None)
                .unwrap();
        }

        Self {
            devices,
            remotes,
            remote_ids,
            media,
            rng: rand::rngs::StdRng::seed_from_u64(seed),
        }
    }

    fn run(mut self) {
        for _ in 0..64 {
            self.run_round();
        }
        self.converge();
        self.verify_each_remote_independently();
    }

    fn run_round(&mut self) {
        let device_index = self.rng.gen_range(0..self.devices.len());
        match self.rng.gen_range(0..4) {
            0 => self.import_and_replicate(device_index),
            1 => {
                let remote_index = self.rng.gen_range(0..self.remote_ids.len());
                self.devices[device_index]
                    .library
                    .fetch_remote(self.remote_ids[remote_index].clone(), None)
                    .unwrap();
            }
            2 => evict_media(&self.devices[device_index], &self.media),
            _ => self.relay_between_remotes(device_index),
        }
    }

    fn import_and_replicate(&mut self, device_index: usize) {
        let device = &mut self.devices[device_index];
        device
            .library
            .fetch_remote(self.remote_ids[0].clone(), None)
            .unwrap();
        assert_media_integrity(device, &self.media);
        self.media.push(device.import_uuid_media());
        device
            .library
            .push_remote(self.remote_ids[0].clone(), None)
            .unwrap();
        for remote_id in self.remote_ids.iter().skip(1) {
            device
                .library
                .push_remote_from_remote(remote_id.clone(), self.remote_ids[0].clone(), None)
                .unwrap();
        }
    }

    fn relay_between_remotes(&mut self, device_index: usize) {
        let source_index = self.rng.gen_range(0..self.remote_ids.len());
        let mut target_index = self.rng.gen_range(0..self.remote_ids.len() - 1);
        if target_index >= source_index {
            target_index += 1;
        }
        self.devices[device_index]
            .library
            .push_remote_from_remote(
                self.remote_ids[target_index].clone(),
                self.remote_ids[source_index].clone(),
                None,
            )
            .unwrap();
    }

    fn converge(&mut self) {
        for device in &self.devices {
            device
                .library
                .fetch_remote(self.remote_ids[0].clone(), None)
                .unwrap();
            assert_media_integrity(device, &self.media);
            device
                .library
                .push_remote(self.remote_ids[0].clone(), None)
                .unwrap();
            for remote_id in self.remote_ids.iter().skip(1) {
                device
                    .library
                    .push_remote_from_remote(remote_id.clone(), self.remote_ids[0].clone(), None)
                    .unwrap();
            }
        }
    }

    fn verify_each_remote_independently(&self) {
        for target_index in 0..self.remotes.len() {
            for (index, remote) in self.remotes.iter().enumerate() {
                remote.set_offline(index != target_index);
            }
            for device in &self.devices {
                evict_media(device, &self.media);
                assert_media_integrity(device, &self.media);
            }
        }
        for remote in &self.remotes {
            remote.set_offline(false);
        }
    }
}
