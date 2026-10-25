//! The room an answer is served in.

use super::*;

#[test]
fn the_object_that_takes_the_total_past_the_room_is_served_and_is_the_last() {
    // 10 + 10 = 20 fits in 25; the third takes it to 30 and is served.
    assert_eq!(window_bounds(&[10, 10, 10, 10], 0, 25), (0, 3));
}
