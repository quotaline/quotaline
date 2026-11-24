use super::*;

#[tokio::test]
async fn distinguishes_a_provider_error_from_a_timeout() {
    let failed = within(
        async { Err::<(), _>(secret_service::Error::Locked) },
        "timeout".into(),
        "failure".into(),
    )
    .await
    .err()
    .unwrap();
    assert_eq!((failed.status, failed.message.as_str()), ("unreadable", "failure"));
    let timed_out = within(
        std::future::pending::<Result<(), _>>(),
        "timeout".into(),
        "failure".into(),
    )
    .await
    .err()
    .unwrap();
    assert_eq!(
        (timed_out.status, timed_out.message.as_str()),
        ("unreadable", "timeout")
    );
}
