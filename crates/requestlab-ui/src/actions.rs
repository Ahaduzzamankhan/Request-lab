//! Application-level actions and their key bindings.

use gpui::{actions, KeyBinding};

actions!(
    requestlab,
    [
        SendRequest,
        NewTab,
        CloseTab,
        SaveRequest,
        ToggleTheme,
        ToggleSidebar,
        CancelRequest,
    ]
);

/// Global shortcuts. Text editing keys are registered separately by the input.
pub fn key_bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("ctrl-enter", SendRequest, None),
        KeyBinding::new("ctrl-t", NewTab, None),
        KeyBinding::new("ctrl-w", CloseTab, None),
        KeyBinding::new("ctrl-s", SaveRequest, None),
        KeyBinding::new("ctrl-shift-l", ToggleTheme, None),
        KeyBinding::new("escape", CancelRequest, None),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcuts_are_registered() {
        let bindings = key_bindings();
        assert_eq!(bindings.len(), 6);
    }
}
