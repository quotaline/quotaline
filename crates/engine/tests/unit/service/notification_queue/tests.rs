use super::*;
use quotaline_notifications::Facts;
use quotaline_notifications::Kind;

fn event(id: &str) -> Planned {
    Planned {
        provider: "Claude".into(),
        event: Event {
            id: id.into(),
            kind: Kind::Credits,
            window_id: None,
            facts: Facts::Credits,
        },
        sign_in: None,
        fetched_at: 0,
    }
}

#[test]
fn queue_keeps_recent_notices_within_capacity() {
    let mut queue = NotificationQueue::<2>::default();
    queue.extend([event("first"), event("second"), event("third")]);
    assert_eq!(
        queue
            .take()
            .into_iter()
            .map(|planned| planned.event.id)
            .collect::<Vec<_>>(),
        ["second", "third"]
    );
}

#[test]
fn taking_notices_clears_the_queue() {
    let mut queue = NotificationQueue::<2>::default();
    queue.extend([event("one")]);
    assert_eq!(queue.take()[0].event.id, "one");
    assert!(queue.take().is_empty());
}
