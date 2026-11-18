use super::*;

#[test]
fn history_selection_returns_the_final_reading_after_queued_events() {
    assert_eq!(step(3, -1, &[1, -1]).unwrap(), 0);
    assert_eq!(step(3, 1, &[1]).unwrap(), 2);
    assert_eq!(step(0, -1, &[1]).unwrap(), -1);
}

#[test]
fn history_selection_rejects_invalid_indices() {
    for (count, selected) in [(-1, -1), (3, -2), (3, 3)] {
        assert!(matches!(step(count, selected, &[1]), Err(Error::InvalidArgs(_))));
    }
}
