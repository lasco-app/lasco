mod actions;
mod assertions;
mod simulator;
mod sync;
mod values;

use proptest::prelude::*;

use simulator::Simulator;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn fuzzy_multi_devices_convergence(seed in any::<u64>()) {
        Simulator::from_seed(seed).run();
    }
}
