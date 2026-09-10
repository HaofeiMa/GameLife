use crate::types::ActivitySeconds;

pub fn sum_activity(slots: &[ActivitySeconds]) -> ActivitySeconds {
    let mut total = ActivitySeconds::default();
    for slot in slots {
        total.add_assign(slot);
    }
    total
}

#[test]
fn mixed_slot_does_not_become_fifteen_core() {
    let slots = vec![ActivitySeconds {
        core: 480,
        side: 420,
        ..Default::default()
    }];
    let s = sum_activity(&slots);
    assert_eq!(s.core, 480);
    assert_eq!(s.side, 420);
    assert_ne!(s.core, 900);
}
