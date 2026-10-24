//! **The contract dealt into parts**: how many, what share each holds, and the
//! test-per-part macro every family that runs the contract in parts shares.

use super::*;

/// How many parts the contract is dealt into: one test each, on a store of its
/// own. At sixteen, the largest part of the real store's folded run stays under a
/// minute, and a fake's store stays small enough that a read through it is cheap.
pub const PARTS: usize = 16;

/// How many cases part `part` of [`PARTS`] holds, of `total` cases.
pub fn part_share(total: usize, part: usize) -> usize {
    let (from, to) = part_bounds(total, part, PARTS);
    to - from
}

/// **A part ran its whole share of the list, and something.** A case dropped
/// from a part, or a part that ran nothing, turns the test of that part red.
pub fn assert_part_ran(what: &str, part: usize, ran: usize, expected: usize) {
    assert!(ran > 0, "part {part} of {what} ran nothing");
    assert_eq!(ran, expected, "part {part} of {what} ran a different share");
}

/// **One test per part**, named by the caller, each running `$run(part)`. A red
/// names the part whose cases it ran. `$attr` is the test attribute, so a family
/// that needs a multi-threaded runtime says so.
#[macro_export]
macro_rules! contract_parts {
    ($attr:meta, $run:path: $($name:ident = $part:literal),+ $(,)?) => {
        $(
            #[$attr]
            async fn $name() {
                $run($part).await;
            }
        )+
    };
}

/// **`of` parts of `total` cases cover every position once**: each part starts
/// where the one before it ended, the first starts at 0, the last ends at
/// `total`, and none is empty. A part that ran nothing would pass its own
/// test, so a run split into one test per part rests on this holding.
pub fn bounds_cover(total: usize, of: usize) {
    let mut next = 0usize;
    for part in 0..of {
        let (from, to) = part_bounds(total, part, of);
        assert_eq!(from, next, "part {part} of {of} leaves a gap or overlaps");
        assert!(to > from, "part {part} of {of} holds no case of {total}");
        next = to;
    }
    assert_eq!(next, total, "{of} parts stop short of {total} cases");
}

/// **The memory contract's `of` parts cover each of its cases once.**
pub fn parts_cover_every_case(of: usize) {
    bounds_cover(case_count(), of);
}
