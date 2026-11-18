use super::*;

#[test]
fn decodes_navigation_and_commands() {
    for (input, actions) in [
        (
            &b"\x1b[A\x1bOBk\t\x1b[Zjrq"[..],
            vec![
                Action::Navigate(Navigation::Up),
                Action::Navigate(Navigation::Down),
                Action::Navigate(Navigation::Up),
                Action::Navigate(Navigation::NextGroup),
                Action::Navigate(Navigation::NextGroup),
                Action::Navigate(Navigation::Down),
                Action::Reload,
                Action::Quit,
            ],
        ),
        (
            &b"1234?d:\rviRs[]n"[..],
            vec![
                Action::Show(Page::Usage),
                Action::Show(Page::History),
                Action::Show(Page::Settings),
                Action::Show(Page::Logs),
                Action::Show(Page::Help),
                Action::Show(Page::Detail),
                Action::StartCommand,
                Action::EditSetting,
                Action::Visibility(false),
                Action::Visibility(true),
                Action::Refresh,
                Action::SignIn,
                Action::History(Some(false)),
                Action::History(Some(true)),
                Action::History(None),
            ],
        ),
        (
            &b"x\x1bk\x1b[15~\x1b[C\x1b[Dq"[..],
            vec![
                Action::Navigate(Navigation::Right),
                Action::Navigate(Navigation::Left),
                Action::Quit,
            ],
        ),
    ] {
        let mut keys = Keys::default();
        assert_eq!(
            input.iter().filter_map(|&byte| keys.push(byte)).collect::<Vec<_>>(),
            actions
        );
    }
}
