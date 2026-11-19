use super::*;
use ratatui::crossterm::event::{KeyEvent, KeyEventState, MouseEvent as Report};

fn key(code: KeyCode, modifiers: KeyModifiers, kind: KeyEventKind) -> Event {
    Event::Key(KeyEvent {
        code,
        modifiers,
        kind,
        state: KeyEventState::NONE,
    })
}

fn mouse(kind: MouseEventKind) -> Event {
    Event::Mouse(Report {
        kind,
        column: 4,
        row: 2,
        modifiers: KeyModifiers::NONE,
    })
}

#[test]
fn encodes_terminal_events_as_the_bytes_the_decoder_reads() {
    let press = |code| key(code, KeyModifiers::NONE, KeyEventKind::Press);
    for (event, motion, bytes) in [
        (
            key(KeyCode::Char('c'), KeyModifiers::CONTROL, KeyEventKind::Press),
            false,
            &b"\x03"[..],
        ),
        (
            key(KeyCode::Char('é'), KeyModifiers::NONE, KeyEventKind::Repeat),
            false,
            "é".as_bytes(),
        ),
        (
            key(KeyCode::Char(':'), KeyModifiers::ALT, KeyEventKind::Press),
            false,
            b"\x1b:",
        ),
        (
            key(KeyCode::Char('q'), KeyModifiers::NONE, KeyEventKind::Release),
            false,
            b"",
        ),
        (press(KeyCode::Enter), false, b"\r"),
        (press(KeyCode::Tab), false, b"\t"),
        (press(KeyCode::BackTab), false, b"\x1b[Z"),
        (press(KeyCode::Backspace), false, b"\x7f"),
        (press(KeyCode::Esc), false, b"\x1b"),
        (press(KeyCode::Up), false, b"\x1b[A"),
        (press(KeyCode::Down), false, b"\x1b[B"),
        (press(KeyCode::Right), false, b"\x1b[C"),
        (press(KeyCode::Left), false, b"\x1b[D"),
        (press(KeyCode::F(1)), false, b""),
        (mouse(MouseEventKind::Down(MouseButton::Left)), false, b"\x1b[<0;5;3M"),
        (mouse(MouseEventKind::Down(MouseButton::Right)), false, b""),
        (mouse(MouseEventKind::Up(MouseButton::Left)), false, b""),
        (mouse(MouseEventKind::Moved), true, b"\x1b[<35;5;3M"),
        (mouse(MouseEventKind::Drag(MouseButton::Left)), true, b"\x1b[<35;5;3M"),
        (mouse(MouseEventKind::Moved), false, b""),
        (mouse(MouseEventKind::Drag(MouseButton::Left)), false, b""),
        (mouse(MouseEventKind::ScrollUp), false, b"\x1b[<64;5;3M"),
        (mouse(MouseEventKind::ScrollDown), false, b"\x1b[<65;5;3M"),
        (mouse(MouseEventKind::ScrollLeft), false, b"\x1b[<66;5;3M"),
        (mouse(MouseEventKind::ScrollRight), false, b"\x1b[<67;5;3M"),
        (Event::Resize(80, 24), false, b""),
    ] {
        assert_eq!(encode(&event, motion), bytes, "{event:?}");
    }
}

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
            input
                .iter()
                .filter_map(|&byte| keys.push(byte, false).0)
                .collect::<Vec<_>>(),
            actions
        );
    }
}

#[test]
fn decodes_fragmented_sgr_mouse_coordinates_and_modifiers() {
    let mut keys = Keys::default();
    for (bytes, expected) in [
        (&b"\x1b[<35;8"[..], Vec::new()),
        (&b";6M"[..], vec![Action::Mouse(MouseEvent::Move(7, 5))]),
        (&b"\x1b[<48;9;7M"[..], vec![Action::Mouse(MouseEvent::Move(8, 6))]),
        (&b"\x1b[<16;1;2M"[..], vec![Action::Mouse(MouseEvent::Click(0, 1))]),
        (
            &b"\x1b[<80;3;7M"[..],
            vec![Action::Mouse(MouseEvent::Scroll(2, 6, Navigation::Up))],
        ),
        (
            &b"\x1b[<65;4;8M"[..],
            vec![Action::Mouse(MouseEvent::Scroll(3, 7, Navigation::Down))],
        ),
        (
            &b"\x1b[<66;5;9M"[..],
            vec![Action::Mouse(MouseEvent::Scroll(4, 8, Navigation::Left))],
        ),
        (
            &b"\x1b[<75;6;10M"[..],
            vec![Action::Mouse(MouseEvent::Scroll(5, 9, Navigation::Right))],
        ),
        (
            &b"\x1b[<0;2;\x1b[<35;7;3M"[..],
            vec![Action::Mouse(MouseEvent::Move(6, 2))],
        ),
    ] {
        assert_eq!(
            bytes
                .iter()
                .filter_map(|&byte| keys.push(byte, false).0)
                .collect::<Vec<_>>(),
            expected
        );
    }
}

#[test]
fn ignores_releases_other_buttons_and_malformed_reports_before_keyboard_input() {
    for report in [
        &b"\x1b[<0;3;7m"[..],
        &b"\x1b[<128;3;7M"[..],
        &b"\x1b[<1;3;7M"[..],
        &b"\x1b[<0;3M"[..],
        &b"\x1b[<0;3;7;5M"[..],
        &b"\x1b[<0;0;7M"[..],
        &b"\x1b[<0;3;0M"[..],
        &b"\x1b[<0;65536;7M"[..],
        &b"\x1b[<0;3;65536M"[..],
        &b"\x1b[<256;2;7M"[..],
        &b"\x1b[<0;2222222222222222222222222;7M"[..],
        &b"\x1b[<0;3;7q"[..],
        &b"\x1b[<0;+3;7M"[..],
        &b"\x1b[<0;\xff3;7M"[..],
        &b"\x1b[<0M"[..],
        &b"\x1b[Mq23"[..],
        &b"\x1bO<0;3;7M"[..],
        &b"\x1bOM"[..],
    ] {
        let mut keys = Keys::default();
        let actions = report
            .iter()
            .chain(b"j")
            .filter_map(|&byte| keys.push(byte, false).0)
            .collect::<Vec<_>>();
        assert_eq!(actions, [Action::Navigate(Navigation::Down)]);
    }
}

#[test]
fn escape_cancellation_keeps_the_following_key() {
    let mut keys = Keys::default();
    assert_eq!(keys.push(b'\x1b', true), (None, false));
    assert_eq!(keys.push(b':', true), (Some(Action::StartCommand), true));
    assert_eq!(keys.push(b'q', true), (Some(Action::Edit(b'q')), false));
}

#[test]
fn only_a_lone_escape_cancels_the_editor_at_a_poll_timeout() {
    let mut keys = Keys::default();
    assert!(!keys.cancel_escape(true));
    assert_eq!(keys.push(b'\x1b', true), (None, false));
    assert!(!keys.cancel_escape(false));
    assert!(keys.cancel_escape(true));
    for &byte in b"\x1b[<35;2;" {
        assert_eq!(keys.push(byte, true), (None, false));
    }
    assert!(!keys.cancel_escape(true));
    assert_eq!(keys.push(b'1', true), (None, false));
    assert_eq!(
        keys.push(b'M', true),
        (Some(Action::Mouse(MouseEvent::Move(1, 0))), false)
    );
}
