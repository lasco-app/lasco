use std::collections::HashSet;

use rand::{Rng as _, SeedableRng as _};

use super::utils;

const ROUND_COUNT: usize = 64;

#[test]
fn one_remote_preserves_operations_across_compaction() {
    let device = utils::Device::new();
    let remote = lasco_core::storage::StorageMockMemoryFaulty::new();
    let remote_id = device.add_remote(&remote);
    let initial_operation_count = device.library.list_operations().unwrap().len();
    let mut rng = rand::rngs::StdRng::seed_from_u64(0x0F5_C0AC_7);
    let mut created_operation_count = 0usize;

    for round in 0..ROUND_COUNT {
        let operation_count = rng.gen_range(1..=30);
        for operation in 0..operation_count {
            device
                .library
                .create_album(format!("round-{round}-operation-{operation}"), None)
                .unwrap();
        }
        created_operation_count += operation_count;
        device.library.push_remote(remote_id.clone(), None).unwrap();
    }

    let operations = device.library.list_operations().unwrap();
    assert_operations_are_complete_and_unique(
        &operations,
        initial_operation_count + created_operation_count,
    );

    let joined_device = utils::Device::join_existing(&device);
    joined_device.register_existing_remote(&remote_id, &remote);
    joined_device.library.fetch_remote(remote_id, None).unwrap();
    let fetched_operations = joined_device.library.list_operations().unwrap();
    assert_operations_are_complete_and_unique(
        &fetched_operations,
        initial_operation_count + created_operation_count,
    );
}

fn assert_operations_are_complete_and_unique(
    operations: &[crate::library::FfiCrdtOperation],
    expected_count: usize,
) {
    let dots: HashSet<_> = operations
        .iter()
        .map(|operation| {
            (
                operation.dot.lamport_counter,
                operation.dot.device_id.as_str(),
            )
        })
        .collect();

    assert_eq!(
        operations.len(),
        expected_count,
        "no local operation may be lost after repeated compaction pushes"
    );
    assert_eq!(
        dots.len(),
        operations.len(),
        "every operation dot must remain unique after repeated compaction pushes"
    );
}
