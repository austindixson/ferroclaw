//! Integration tests for the TUI application state.

use ferroclaw::tui::app::{App, ChatEntry};

#[test]
fn test_app_initial_state() {
    let app = App::new("claude-sonnet-4-20250514".into(), 200_000);
    assert!(app.chat_history.is_empty());
    assert_eq!(app.input_lines.len(), 1);
    assert_eq!(app.input_lines[0], "");
    assert_eq!(app.cursor_line, 0);
    assert_eq!(app.cursor_col, 0);
    assert_eq!(app.scroll_offset, 0);
    assert_eq!(app.tokens_used, 0);
    assert_eq!(app.status, "Ready");
}

#[test]
fn test_app_input_and_take() {
    let mut app = App::new("test".into(), 100_000);
    app.input_char('H');
    app.input_char('e');
    app.input_char('l');
    app.input_char('l');
    app.input_char('o');
    assert_eq!(app.input_text(), "Hello");
    assert_eq!(app.cursor_col, 5);

    let taken = app.take_input();
    assert_eq!(taken, "Hello");
    assert_eq!(app.input_text(), "");
    assert_eq!(app.cursor_col, 0);
}

#[test]
fn test_app_multiline_input() {
    let mut app = App::new("test".into(), 100_000);
    app.input_char('a');
    app.input_newline();
    app.input_char('b');
    app.input_newline();
    app.input_char('c');

    assert_eq!(app.input_lines.len(), 3);
    assert_eq!(app.input_text(), "a\nb\nc");
    assert_eq!(app.cursor_line, 2);
    assert_eq!(app.cursor_col, 1);
}

#[test]
fn test_app_backspace_merges_lines() {
    let mut app = App::new("test".into(), 100_000);
    app.input_char('a');
    app.input_newline();
    app.input_char('b');
    assert_eq!(app.input_lines.len(), 2);

    // Move to start of line 1 and backspace to merge
    app.input_home();
    app.input_backspace();
    assert_eq!(app.input_lines.len(), 1);
    assert_eq!(app.input_text(), "ab");
}

#[test]
fn test_app_delete_key() {
    let mut app = App::new("test".into(), 100_000);
    app.input_char('a');
    app.input_char('b');
    app.input_char('c');
    app.input_home();
    app.input_delete();
    assert_eq!(app.input_text(), "bc");
}

#[test]
fn test_app_cursor_movement() {
    let mut app = App::new("test".into(), 100_000);
    app.input_char('a');
    app.input_char('b');
    app.input_char('c');
    assert_eq!(app.cursor_col, 3);

    app.input_move_left();
    assert_eq!(app.cursor_col, 2);

    app.input_home();
    assert_eq!(app.cursor_col, 0);

    app.input_end();
    assert_eq!(app.cursor_col, 3);

    app.input_move_right(); // Should not go beyond end
    assert_eq!(app.cursor_col, 3);
}

#[test]
fn test_app_cursor_vertical_movement() {
    let mut app = App::new("test".into(), 100_000);
    app.input_char('a');
    app.input_char('b');
    app.input_newline();
    app.input_char('c');
    assert_eq!(app.cursor_line, 1);

    app.input_move_up();
    assert_eq!(app.cursor_line, 0);

    app.input_move_down();
    assert_eq!(app.cursor_line, 1);

    // Should not go below last line
    app.input_move_down();
    assert_eq!(app.cursor_line, 1);
}

#[test]
fn test_app_scroll_bounds() {
    let mut app = App::new("test".into(), 100_000);
    app.total_chat_lines = 50;
    app.visible_chat_height = 20;

    // Scroll up to max
    app.scroll_up(100);
    assert_eq!(app.scroll_offset, 30); // max = 50 - 20

    // Scroll down past zero
    app.scroll_down(100);
    assert_eq!(app.scroll_offset, 0);
}

#[test]
fn test_app_clear_chat() {
    let mut app = App::new("test".into(), 100_000);
    app.chat_history
        .push(ChatEntry::UserMessage("hello".into()));
    app.chat_history
        .push(ChatEntry::AssistantMessage("hi".into()));
    app.scroll_offset = 5;

    app.clear_chat();
    assert_eq!(app.chat_history.len(), 1);
    assert!(matches!(&app.chat_history[0], ChatEntry::SystemInfo(_)));
    assert_eq!(app.scroll_offset, 0);
}

#[test]
fn test_app_set_status() {
    let mut app = App::new("test".into(), 100_000);
    assert_eq!(app.status, "Ready");
    app.set_status("Thinking...");
    assert_eq!(app.status, "Thinking...");
}

#[test]
fn test_app_take_input_trims_whitespace() {
    let mut app = App::new("test".into(), 100_000);
    app.input_char(' ');
    app.input_char(' ');
    app.input_char('h');
    app.input_char('i');
    app.input_char(' ');
    let taken = app.take_input();
    assert_eq!(taken, "hi");
}

#[test]
fn test_chat_entry_variants() {
    let entries = [
        ChatEntry::UserMessage("hello".into()),
        ChatEntry::AssistantMessage("hi".into()),
        ChatEntry::ToolCall {
            name: "read_file".into(),
            args: "{}".into(),
        },
        ChatEntry::ToolResult {
            name: "read_file".into(),
            content: "data".into(),
            is_error: false,
        },
        ChatEntry::SystemInfo("System started".into()),
        ChatEntry::Error("Something went wrong".into()),
    ];
    assert_eq!(entries.len(), 6);
}
