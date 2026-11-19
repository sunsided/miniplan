//! Fuzzes the full pipeline on untrusted PDDL: parse, ground, then a bounded search.
//!
//! Any input may be rejected with an error, but nothing may panic or hang.

#![no_main]

use libfuzzer_sys::fuzz_target;
use miniplan::ground::ground;
use miniplan::pddl_io::load_combined_str;
use miniplan::search::{Bfs, Planner, SearchLimits};

/// Node budget for the search, so a large valid task cannot stall the fuzzer.
const NODE_BUDGET: u64 = 1_000;

fuzz_target!(|data: &[u8]| {
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
        node_budget: Some(NODE_BUDGET),
        ..SearchLimits::default()
    };
    let _ = Bfs::new().solve(&task, &limits);
});
