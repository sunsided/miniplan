//! Fuzzes the full pipeline on untrusted PDDL: parse, ground, then a bounded search.
//!
//! Any input may be rejected with an error, but nothing may panic. Grounding has no size cap
//! of its own, so a small input with many objects and high-arity actions can still take long
//! to ground; libFuzzer's `-timeout` reports those as slow inputs.

#![no_main]

use std::time::Duration;

use libfuzzer_sys::fuzz_target;
use miniplan::ground::ground;
use miniplan::pddl_io::load_combined_str;
use miniplan::search::{Bfs, Planner, SearchLimits};

/// Largest input in bytes. Longer inputs mostly add objects, which grow grounding
/// combinatorially without reaching new code.
const MAX_INPUT_LEN: usize = 4096;

/// Node budget for the search, so a large valid task cannot stall the fuzzer.
const NODE_BUDGET: u64 = 1_000;

/// Time budget for the search, replacing the 300 s default of [`SearchLimits`].
const TIME_BUDGET: Duration = Duration::from_secs(1);

fuzz_target!(|data: &[u8]| {
    if data.len() > MAX_INPUT_LEN {
        return;
    }

    let Ok(source) = std::str::from_utf8(data) else {
        return;
    };
    let Ok((domain, problem)) = load_combined_str(source) else {
        return;
    };
    let Ok(task) = ground(&domain, &problem) else {
        return;
    };

    let limits = SearchLimits {
        time_budget: Some(TIME_BUDGET),
        node_budget: Some(NODE_BUDGET),
        ..SearchLimits::default()
    };
    let _ = Bfs::new().solve(&task, &limits);
});
