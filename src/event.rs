// Keyboard event handling and dispatch

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::AppMode;

/// Represents all possible actions the app can take in response to input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Quit,
    MoveUp,
    MoveDown,
    Enter,
    GoParent,
    OpenFile,
    CopyFile,
    ToggleHidden,
    StartLocalFind,
    StartGlobalFind,
    StartNavigate,
    ConfirmInput,
    CancelInput,
    CharInput(char),
    Backspace,
    AbortSearch,
    ConfirmOverwrite,
    DenyOverwrite,
    None,
}

/// Map a key event to an Action based on the current application mode.
pub fn handle_key_event(key: KeyEvent, mode: &AppMode) -> Action {
    match mode {
        AppMode::Browsing => handle_browsing(key),
        AppMode::SearchPrompt { .. } => handle_text_input(key),
        AppMode::SearchResults => handle_search_results(key),
        AppMode::PathPrompt => handle_text_input(key),
        AppMode::OverwriteConfirm { .. } => handle_overwrite_confirm(key),
        AppMode::NavigatePrompt => handle_text_input(key),
        AppMode::Searching { .. } => handle_searching(key),
        AppMode::Copying { .. } => Action::None,
    }
}

fn handle_browsing(key: KeyEvent) -> Action {
    // Check for Ctrl-modified keys first
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        return match key.code {
            KeyCode::Char('f') => Action::StartLocalFind,
            KeyCode::Char('g') => Action::StartGlobalFind,
            _ => Action::None,
        };
    }

    match key.code {
        KeyCode::Down | KeyCode::Char('j') => Action::MoveDown,
        KeyCode::Up | KeyCode::Char('k') => Action::MoveUp,
        KeyCode::Enter => Action::Enter,
        KeyCode::Backspace | KeyCode::Char('-') => Action::GoParent,
        KeyCode::Char('o') => Action::OpenFile,
        KeyCode::Char('c') => Action::CopyFile,
        KeyCode::Char('q') => Action::Quit,
        KeyCode::Char('a') => Action::ToggleHidden,
        KeyCode::Char('m') => Action::StartNavigate,
        _ => Action::None,
    }
}

fn handle_text_input(key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Esc => Action::CancelInput,
        KeyCode::Enter => Action::ConfirmInput,
        KeyCode::Backspace => Action::Backspace,
        KeyCode::Char(c) => Action::CharInput(c),
        _ => Action::None,
    }
}

fn handle_search_results(key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Down | KeyCode::Char('j') => Action::MoveDown,
        KeyCode::Up | KeyCode::Char('k') => Action::MoveUp,
        KeyCode::Enter => Action::Enter,
        KeyCode::Esc => Action::CancelInput,
        _ => Action::None,
    }
}

fn handle_overwrite_confirm(key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Char('y') => Action::ConfirmOverwrite,
        KeyCode::Char('n') | KeyCode::Esc => Action::DenyOverwrite,
        _ => Action::None,
    }
}

fn handle_searching(key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Esc => Action::AbortSearch,
        _ => Action::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    fn key_ctrl(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::CONTROL,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    // --- Browsing mode tests ---

    #[test]
    fn browsing_arrow_down() {
        assert_eq!(handle_key_event(key(KeyCode::Down), &AppMode::Browsing), Action::MoveDown);
    }

    #[test]
    fn browsing_j_down() {
        assert_eq!(handle_key_event(key(KeyCode::Char('j')), &AppMode::Browsing), Action::MoveDown);
    }

    #[test]
    fn browsing_arrow_up() {
        assert_eq!(handle_key_event(key(KeyCode::Up), &AppMode::Browsing), Action::MoveUp);
    }

    #[test]
    fn browsing_k_up() {
        assert_eq!(handle_key_event(key(KeyCode::Char('k')), &AppMode::Browsing), Action::MoveUp);
    }

    #[test]
    fn browsing_enter() {
        assert_eq!(handle_key_event(key(KeyCode::Enter), &AppMode::Browsing), Action::Enter);
    }

    #[test]
    fn browsing_backspace_parent() {
        assert_eq!(handle_key_event(key(KeyCode::Backspace), &AppMode::Browsing), Action::GoParent);
    }

    #[test]
    fn browsing_dash_parent() {
        assert_eq!(handle_key_event(key(KeyCode::Char('-')), &AppMode::Browsing), Action::GoParent);
    }

    #[test]
    fn browsing_o_open_file() {
        assert_eq!(handle_key_event(key(KeyCode::Char('o')), &AppMode::Browsing), Action::OpenFile);
    }

    #[test]
    fn browsing_c_copy_file() {
        assert_eq!(handle_key_event(key(KeyCode::Char('c')), &AppMode::Browsing), Action::CopyFile);
    }

    #[test]
    fn browsing_q_quit() {
        assert_eq!(handle_key_event(key(KeyCode::Char('q')), &AppMode::Browsing), Action::Quit);
    }

    #[test]
    fn browsing_a_toggle_hidden() {
        assert_eq!(handle_key_event(key(KeyCode::Char('a')), &AppMode::Browsing), Action::ToggleHidden);
    }

    #[test]
    fn browsing_ctrl_f_local_find() {
        assert_eq!(handle_key_event(key_ctrl(KeyCode::Char('f')), &AppMode::Browsing), Action::StartLocalFind);
    }

    #[test]
    fn browsing_ctrl_g_global_find() {
        assert_eq!(handle_key_event(key_ctrl(KeyCode::Char('g')), &AppMode::Browsing), Action::StartGlobalFind);
    }

    #[test]
    fn browsing_m_navigate() {
        assert_eq!(handle_key_event(key(KeyCode::Char('m')), &AppMode::Browsing), Action::StartNavigate);
    }

    #[test]
    fn browsing_unknown_key_none() {
        assert_eq!(handle_key_event(key(KeyCode::Char('z')), &AppMode::Browsing), Action::None);
    }

    // --- SearchPrompt mode tests ---

    #[test]
    fn search_prompt_escape() {
        let mode = AppMode::SearchPrompt { search_type: crate::app::SearchType::Local };
        assert_eq!(handle_key_event(key(KeyCode::Esc), &mode), Action::CancelInput);
    }

    #[test]
    fn search_prompt_enter() {
        let mode = AppMode::SearchPrompt { search_type: crate::app::SearchType::Local };
        assert_eq!(handle_key_event(key(KeyCode::Enter), &mode), Action::ConfirmInput);
    }

    #[test]
    fn search_prompt_backspace() {
        let mode = AppMode::SearchPrompt { search_type: crate::app::SearchType::Global };
        assert_eq!(handle_key_event(key(KeyCode::Backspace), &mode), Action::Backspace);
    }

    #[test]
    fn search_prompt_char_input() {
        let mode = AppMode::SearchPrompt { search_type: crate::app::SearchType::Local };
        assert_eq!(handle_key_event(key(KeyCode::Char('x')), &mode), Action::CharInput('x'));
    }

    // --- SearchResults mode tests ---

    #[test]
    fn search_results_down() {
        assert_eq!(handle_key_event(key(KeyCode::Down), &AppMode::SearchResults), Action::MoveDown);
    }

    #[test]
    fn search_results_j_down() {
        assert_eq!(handle_key_event(key(KeyCode::Char('j')), &AppMode::SearchResults), Action::MoveDown);
    }

    #[test]
    fn search_results_up() {
        assert_eq!(handle_key_event(key(KeyCode::Up), &AppMode::SearchResults), Action::MoveUp);
    }

    #[test]
    fn search_results_k_up() {
        assert_eq!(handle_key_event(key(KeyCode::Char('k')), &AppMode::SearchResults), Action::MoveUp);
    }

    #[test]
    fn search_results_enter() {
        assert_eq!(handle_key_event(key(KeyCode::Enter), &AppMode::SearchResults), Action::Enter);
    }

    #[test]
    fn search_results_escape() {
        assert_eq!(handle_key_event(key(KeyCode::Esc), &AppMode::SearchResults), Action::CancelInput);
    }

    // --- PathPrompt mode tests ---

    #[test]
    fn path_prompt_escape() {
        assert_eq!(handle_key_event(key(KeyCode::Esc), &AppMode::PathPrompt), Action::CancelInput);
    }

    #[test]
    fn path_prompt_enter() {
        assert_eq!(handle_key_event(key(KeyCode::Enter), &AppMode::PathPrompt), Action::ConfirmInput);
    }

    #[test]
    fn path_prompt_backspace() {
        assert_eq!(handle_key_event(key(KeyCode::Backspace), &AppMode::PathPrompt), Action::Backspace);
    }

    #[test]
    fn path_prompt_char() {
        assert_eq!(handle_key_event(key(KeyCode::Char('/')), &AppMode::PathPrompt), Action::CharInput('/'));
    }

    // --- OverwriteConfirm mode tests ---

    #[test]
    fn overwrite_y_confirm() {
        let mode = AppMode::OverwriteConfirm { path: std::path::PathBuf::from("/tmp/file") };
        assert_eq!(handle_key_event(key(KeyCode::Char('y')), &mode), Action::ConfirmOverwrite);
    }

    #[test]
    fn overwrite_n_deny() {
        let mode = AppMode::OverwriteConfirm { path: std::path::PathBuf::from("/tmp/file") };
        assert_eq!(handle_key_event(key(KeyCode::Char('n')), &mode), Action::DenyOverwrite);
    }

    #[test]
    fn overwrite_escape_deny() {
        let mode = AppMode::OverwriteConfirm { path: std::path::PathBuf::from("/tmp/file") };
        assert_eq!(handle_key_event(key(KeyCode::Esc), &mode), Action::DenyOverwrite);
    }

    #[test]
    fn overwrite_other_none() {
        let mode = AppMode::OverwriteConfirm { path: std::path::PathBuf::from("/tmp/file") };
        assert_eq!(handle_key_event(key(KeyCode::Char('x')), &mode), Action::None);
    }

    // --- NavigatePrompt mode tests ---

    #[test]
    fn navigate_prompt_escape() {
        assert_eq!(handle_key_event(key(KeyCode::Esc), &AppMode::NavigatePrompt), Action::CancelInput);
    }

    #[test]
    fn navigate_prompt_enter() {
        assert_eq!(handle_key_event(key(KeyCode::Enter), &AppMode::NavigatePrompt), Action::ConfirmInput);
    }

    #[test]
    fn navigate_prompt_backspace() {
        assert_eq!(handle_key_event(key(KeyCode::Backspace), &AppMode::NavigatePrompt), Action::Backspace);
    }

    #[test]
    fn navigate_prompt_char() {
        assert_eq!(handle_key_event(key(KeyCode::Char('a')), &AppMode::NavigatePrompt), Action::CharInput('a'));
    }

    // --- Searching mode tests ---

    #[test]
    fn searching_escape_abort() {
        let mode = AppMode::Searching { search_type: crate::app::SearchType::Local, progress: 0 };
        assert_eq!(handle_key_event(key(KeyCode::Esc), &mode), Action::AbortSearch);
    }

    #[test]
    fn searching_other_none() {
        let mode = AppMode::Searching { search_type: crate::app::SearchType::Global, progress: 5 };
        assert_eq!(handle_key_event(key(KeyCode::Char('x')), &mode), Action::None);
    }

    // --- Copying mode tests ---

    #[test]
    fn copying_ignores_all_keys() {
        let mode = AppMode::Copying { bytes_transferred: 100 };
        assert_eq!(handle_key_event(key(KeyCode::Char('q')), &mode), Action::None);
        assert_eq!(handle_key_event(key(KeyCode::Esc), &mode), Action::None);
        assert_eq!(handle_key_event(key(KeyCode::Enter), &mode), Action::None);
    }
}
