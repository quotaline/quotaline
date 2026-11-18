use super::*;

#[test]
fn decodes_navigation_and_commands() {
    for (input, actions) in [
        (
            &b"\x1b[A\x1bOBk\t\x1b[Zjrq\x03"[..],
            vec![
                Action::Navigate(Navigation::Up),
                Action::Navigate(Navigation::Down),
                Action::Navigate(Navigation::Up),
                Action::Navigate(Navigation::TogglePanel),
                Action::Navigate(Navigation::TogglePanel),
                Action::Navigate(Navigation::Down),
                Action::Reload,
                Action::Quit,
                Action::Quit,
            ],
        ),
        (&b"x\x1bk\x1b[15~\x1b[Cq"[..], vec![Action::Quit]),
    ] {
        let mut keys = Keys::default();
        assert_eq!(
            input.iter().filter_map(|&byte| keys.push(byte)).collect::<Vec<_>>(),
            actions
        );
    }
}
