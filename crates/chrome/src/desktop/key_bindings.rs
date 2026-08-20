/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::sync::{OnceLock, RwLock};

use keyboard_types::{Code, Key, KeyboardEvent, Modifiers, NamedKey};
use log::warn;
use serde_json::json;
use slate_storage::SlateProfileDatabase;
use url::Url;

use super::keyutils::CMD_OR_CONTROL;

const KEY_BINDING_ACTIONS: [KeyBindingAction; 10] = [
    KeyBindingAction::NewTab,
    KeyBindingAction::CloseTab,
    KeyBindingAction::NextTab,
    KeyBindingAction::PreviousTab,
    KeyBindingAction::NextApp,
    KeyBindingAction::PreviousApp,
    KeyBindingAction::Cut,
    KeyBindingAction::Copy,
    KeyBindingAction::Paste,
    KeyBindingAction::SelectAll,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum KeyBindingAction {
    NewTab,
    CloseTab,
    NextTab,
    PreviousTab,
    NextApp,
    PreviousApp,
    Cut,
    Copy,
    Paste,
    SelectAll,
}

impl KeyBindingAction {
    fn id(self) -> &'static str {
        match self {
            Self::NewTab => "new_tab",
            Self::CloseTab => "close_tab",
            Self::NextTab => "next_tab",
            Self::PreviousTab => "previous_tab",
            Self::NextApp => "next_app",
            Self::PreviousApp => "previous_app",
            Self::Cut => "cut",
            Self::Copy => "copy",
            Self::Paste => "paste",
            Self::SelectAll => "select_all",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::NewTab => "New tab",
            Self::CloseTab => "Close tab",
            Self::NextTab => "Next tab",
            Self::PreviousTab => "Previous tab",
            Self::NextApp => "Next app",
            Self::PreviousApp => "Previous app",
            Self::Cut => "Cut",
            Self::Copy => "Copy",
            Self::Paste => "Paste",
            Self::SelectAll => "Select all",
        }
    }

    fn setting_key(self) -> &'static str {
        match self {
            Self::NewTab => "keybindings.new_tab",
            Self::CloseTab => "keybindings.close_tab",
            Self::NextTab => "keybindings.next_tab",
            Self::PreviousTab => "keybindings.previous_tab",
            Self::NextApp => "keybindings.next_app",
            Self::PreviousApp => "keybindings.previous_app",
            Self::Cut => "keybindings.cut",
            Self::Copy => "keybindings.copy",
            Self::Paste => "keybindings.paste",
            Self::SelectAll => "keybindings.select_all",
        }
    }

    fn query_parameter(self) -> &'static str {
        match self {
            Self::NewTab => "key_new_tab",
            Self::CloseTab => "key_close_tab",
            Self::NextTab => "key_next_tab",
            Self::PreviousTab => "key_previous_tab",
            Self::NextApp => "key_next_app",
            Self::PreviousApp => "key_previous_app",
            Self::Cut => "key_cut",
            Self::Copy => "key_copy",
            Self::Paste => "key_paste",
            Self::SelectAll => "key_select_all",
        }
    }

    fn default_setting_value(self) -> &'static str {
        match self {
            Self::NewTab => "Primary+T",
            Self::CloseTab => "Primary+W",
            Self::NextTab => "Ctrl+Tab",
            Self::PreviousTab => "Ctrl+Shift+Tab",
            Self::NextApp => "Ctrl+'",
            Self::PreviousApp => "Ctrl+Shift+'",
            Self::Cut => "Primary+X",
            Self::Copy => "Primary+C",
            Self::Paste => "Primary+V",
            Self::SelectAll => "Primary+A",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SlateKeyBindings {
    new_tab: KeyBinding,
    close_tab: KeyBinding,
    next_tab: KeyBinding,
    previous_tab: KeyBinding,
    next_app: KeyBinding,
    previous_app: KeyBinding,
    cut: KeyBinding,
    copy: KeyBinding,
    paste: KeyBinding,
    select_all: KeyBinding,
}

impl Default for SlateKeyBindings {
    fn default() -> Self {
        Self {
            new_tab: default_key_binding(KeyBindingAction::NewTab),
            close_tab: default_key_binding(KeyBindingAction::CloseTab),
            next_tab: default_key_binding(KeyBindingAction::NextTab),
            previous_tab: default_key_binding(KeyBindingAction::PreviousTab),
            next_app: default_key_binding(KeyBindingAction::NextApp),
            previous_app: default_key_binding(KeyBindingAction::PreviousApp),
            cut: default_key_binding(KeyBindingAction::Cut),
            copy: default_key_binding(KeyBindingAction::Copy),
            paste: default_key_binding(KeyBindingAction::Paste),
            select_all: default_key_binding(KeyBindingAction::SelectAll),
        }
    }
}

impl SlateKeyBindings {
    fn binding(&self, action: KeyBindingAction) -> &KeyBinding {
        match action {
            KeyBindingAction::NewTab => &self.new_tab,
            KeyBindingAction::CloseTab => &self.close_tab,
            KeyBindingAction::NextTab => &self.next_tab,
            KeyBindingAction::PreviousTab => &self.previous_tab,
            KeyBindingAction::NextApp => &self.next_app,
            KeyBindingAction::PreviousApp => &self.previous_app,
            KeyBindingAction::Cut => &self.cut,
            KeyBindingAction::Copy => &self.copy,
            KeyBindingAction::Paste => &self.paste,
            KeyBindingAction::SelectAll => &self.select_all,
        }
    }

    fn set_binding(&mut self, action: KeyBindingAction, binding: KeyBinding) {
        match action {
            KeyBindingAction::NewTab => self.new_tab = binding,
            KeyBindingAction::CloseTab => self.close_tab = binding,
            KeyBindingAction::NextTab => self.next_tab = binding,
            KeyBindingAction::PreviousTab => self.previous_tab = binding,
            KeyBindingAction::NextApp => self.next_app = binding,
            KeyBindingAction::PreviousApp => self.previous_app = binding,
            KeyBindingAction::Cut => self.cut = binding,
            KeyBindingAction::Copy => self.copy = binding,
            KeyBindingAction::Paste => self.paste = binding,
            KeyBindingAction::SelectAll => self.select_all = binding,
        }
    }

    fn action_for_event(&self, event: &KeyboardEvent) -> Option<KeyBindingAction> {
        KEY_BINDING_ACTIONS
            .iter()
            .copied()
            .find(|action| self.binding(*action).matches_event(event))
    }

    fn json_value(&self) -> serde_json::Value {
        let bindings: Vec<_> = KEY_BINDING_ACTIONS
            .iter()
            .copied()
            .map(|action| {
                let binding = self.binding(action);
                json!({
                    "id": action.id(),
                    "label": action.label(),
                    "query": action.query_parameter(),
                    "value": binding.setting_value(),
                    "display": binding.display_label(),
                    "default_value": action.default_setting_value(),
                    "default_display": default_key_binding(action).display_label(),
                })
            })
            .collect();
        json!(bindings)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct KeyBinding {
    modifiers: Modifiers,
    key: BindingKey,
}

impl KeyBinding {
    fn character(modifiers: Modifiers, key: char) -> Self {
        Self {
            modifiers: modifiers & modifier_mask(),
            key: BindingKey::Character(key.to_ascii_uppercase()),
        }
    }

    fn named(modifiers: Modifiers, key: NamedKey) -> Self {
        Self {
            modifiers: modifiers & modifier_mask(),
            key: BindingKey::Named(key),
        }
    }

    fn parse(value: &str) -> Option<Self> {
        let mut modifiers = Modifiers::empty();
        let mut key = None;

        for token in value
            .split('+')
            .map(str::trim)
            .filter(|token| !token.is_empty())
        {
            if let Some(modifier) = modifier_from_token(token) {
                modifiers |= modifier;
                continue;
            }

            if key.is_some() {
                return None;
            }
            key = binding_key_from_token(token);
        }

        key.map(|key| Self {
            modifiers: modifiers & modifier_mask(),
            key,
        })
    }

    fn matches_event(&self, event: &KeyboardEvent) -> bool {
        let modifiers = event.modifiers & modifier_mask();
        modifiers == self.modifiers && self.key.matches_event(event)
    }

    fn setting_value(&self) -> String {
        let mut tokens = Vec::new();
        if self.modifiers.contains(Modifiers::CONTROL) {
            tokens.push("Ctrl".to_string());
        }
        if self.modifiers.contains(Modifiers::SHIFT) {
            tokens.push("Shift".to_string());
        }
        if self.modifiers.contains(Modifiers::ALT) {
            tokens.push("Alt".to_string());
        }
        if self.modifiers.contains(Modifiers::META) {
            tokens.push("Meta".to_string());
        }
        tokens.push(self.key.label());
        tokens.join("+")
    }

    fn display_label(&self) -> String {
        self.setting_value()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum BindingKey {
    Character(char),
    Named(NamedKey),
}

impl BindingKey {
    fn matches_event(&self, event: &KeyboardEvent) -> bool {
        match (self, &event.key) {
            (Self::Character(expected), Key::Character(actual)) => {
                let mut buffer = [0; 4];
                actual.eq_ignore_ascii_case(expected.encode_utf8(&mut buffer))
                    || self.matches_physical_code(event)
            }
            (Self::Named(expected), Key::Named(actual)) => expected == actual,
            _ => self.matches_physical_code(event),
        }
    }

    fn matches_physical_code(&self, event: &KeyboardEvent) -> bool {
        matches!(self, Self::Character('\'')) && event.code == Code::Quote
    }

    fn label(&self) -> String {
        match self {
            Self::Character(key) => key.to_string(),
            Self::Named(key) => named_key_label(key).to_string(),
        }
    }
}

fn modifier_from_token(token: &str) -> Option<Modifiers> {
    if token.eq_ignore_ascii_case("primary") {
        return Some(CMD_OR_CONTROL);
    }
    if token.eq_ignore_ascii_case("ctrl") || token.eq_ignore_ascii_case("control") {
        return Some(Modifiers::CONTROL);
    }
    if token.eq_ignore_ascii_case("shift") {
        return Some(Modifiers::SHIFT);
    }
    if token.eq_ignore_ascii_case("alt") || token.eq_ignore_ascii_case("option") {
        return Some(Modifiers::ALT);
    }
    if token.eq_ignore_ascii_case("meta")
        || token.eq_ignore_ascii_case("cmd")
        || token.eq_ignore_ascii_case("command")
        || token.eq_ignore_ascii_case("super")
    {
        return Some(Modifiers::META);
    }
    None
}

fn binding_key_from_token(token: &str) -> Option<BindingKey> {
    let normalized = token.trim();
    if normalized.chars().count() == 1 {
        return normalized
            .chars()
            .next()
            .map(|key| BindingKey::Character(key.to_ascii_uppercase()));
    }

    named_key_from_token(normalized).map(BindingKey::Named)
}

fn named_key_from_token(token: &str) -> Option<NamedKey> {
    if let Some(function_key) = function_key_from_token(token) {
        return Some(function_key);
    }

    if token.eq_ignore_ascii_case("PageUp") || token.eq_ignore_ascii_case("Page Up") {
        return Some(NamedKey::PageUp);
    }
    if token.eq_ignore_ascii_case("PageDown") || token.eq_ignore_ascii_case("Page Down") {
        return Some(NamedKey::PageDown);
    }
    if token.eq_ignore_ascii_case("ArrowLeft") || token.eq_ignore_ascii_case("Left") {
        return Some(NamedKey::ArrowLeft);
    }
    if token.eq_ignore_ascii_case("ArrowRight") || token.eq_ignore_ascii_case("Right") {
        return Some(NamedKey::ArrowRight);
    }
    if token.eq_ignore_ascii_case("ArrowUp") || token.eq_ignore_ascii_case("Up") {
        return Some(NamedKey::ArrowUp);
    }
    if token.eq_ignore_ascii_case("ArrowDown") || token.eq_ignore_ascii_case("Down") {
        return Some(NamedKey::ArrowDown);
    }
    if token.eq_ignore_ascii_case("Tab") {
        return Some(NamedKey::Tab);
    }
    if token.eq_ignore_ascii_case("Escape") || token.eq_ignore_ascii_case("Esc") {
        return Some(NamedKey::Escape);
    }
    if token.eq_ignore_ascii_case("Enter") || token.eq_ignore_ascii_case("Return") {
        return Some(NamedKey::Enter);
    }
    if token.eq_ignore_ascii_case("Backspace") {
        return Some(NamedKey::Backspace);
    }
    if token.eq_ignore_ascii_case("Delete") || token.eq_ignore_ascii_case("Del") {
        return Some(NamedKey::Delete);
    }
    if token.eq_ignore_ascii_case("Insert") || token.eq_ignore_ascii_case("Ins") {
        return Some(NamedKey::Insert);
    }
    if token.eq_ignore_ascii_case("Home") {
        return Some(NamedKey::Home);
    }
    if token.eq_ignore_ascii_case("End") {
        return Some(NamedKey::End);
    }
    None
}

fn function_key_from_token(token: &str) -> Option<NamedKey> {
    let number = token
        .strip_prefix('F')
        .or_else(|| token.strip_prefix('f'))?
        .parse::<u8>()
        .ok()?;
    match number {
        1 => Some(NamedKey::F1),
        2 => Some(NamedKey::F2),
        3 => Some(NamedKey::F3),
        4 => Some(NamedKey::F4),
        5 => Some(NamedKey::F5),
        6 => Some(NamedKey::F6),
        7 => Some(NamedKey::F7),
        8 => Some(NamedKey::F8),
        9 => Some(NamedKey::F9),
        10 => Some(NamedKey::F10),
        11 => Some(NamedKey::F11),
        12 => Some(NamedKey::F12),
        13 => Some(NamedKey::F13),
        14 => Some(NamedKey::F14),
        15 => Some(NamedKey::F15),
        16 => Some(NamedKey::F16),
        17 => Some(NamedKey::F17),
        18 => Some(NamedKey::F18),
        19 => Some(NamedKey::F19),
        20 => Some(NamedKey::F20),
        21 => Some(NamedKey::F21),
        22 => Some(NamedKey::F22),
        23 => Some(NamedKey::F23),
        24 => Some(NamedKey::F24),
        _ => None,
    }
}

fn named_key_label(key: &NamedKey) -> &'static str {
    match key {
        NamedKey::PageUp => "PageUp",
        NamedKey::PageDown => "PageDown",
        NamedKey::ArrowLeft => "ArrowLeft",
        NamedKey::ArrowRight => "ArrowRight",
        NamedKey::ArrowUp => "ArrowUp",
        NamedKey::ArrowDown => "ArrowDown",
        NamedKey::Tab => "Tab",
        NamedKey::Escape => "Escape",
        NamedKey::Enter => "Enter",
        NamedKey::Backspace => "Backspace",
        NamedKey::Delete => "Delete",
        NamedKey::Insert => "Insert",
        NamedKey::Home => "Home",
        NamedKey::End => "End",
        NamedKey::F1 => "F1",
        NamedKey::F2 => "F2",
        NamedKey::F3 => "F3",
        NamedKey::F4 => "F4",
        NamedKey::F5 => "F5",
        NamedKey::F6 => "F6",
        NamedKey::F7 => "F7",
        NamedKey::F8 => "F8",
        NamedKey::F9 => "F9",
        NamedKey::F10 => "F10",
        NamedKey::F11 => "F11",
        NamedKey::F12 => "F12",
        NamedKey::F13 => "F13",
        NamedKey::F14 => "F14",
        NamedKey::F15 => "F15",
        NamedKey::F16 => "F16",
        NamedKey::F17 => "F17",
        NamedKey::F18 => "F18",
        NamedKey::F19 => "F19",
        NamedKey::F20 => "F20",
        NamedKey::F21 => "F21",
        NamedKey::F22 => "F22",
        NamedKey::F23 => "F23",
        NamedKey::F24 => "F24",
        _ => "Unidentified",
    }
}

fn default_key_binding(action: KeyBindingAction) -> KeyBinding {
    match action {
        KeyBindingAction::NewTab => KeyBinding::character(CMD_OR_CONTROL, 'T'),
        KeyBindingAction::CloseTab => KeyBinding::character(CMD_OR_CONTROL, 'W'),
        KeyBindingAction::NextTab => KeyBinding::named(Modifiers::CONTROL, NamedKey::Tab),
        KeyBindingAction::PreviousTab => {
            KeyBinding::named(Modifiers::CONTROL | Modifiers::SHIFT, NamedKey::Tab)
        }
        KeyBindingAction::NextApp => KeyBinding::character(Modifiers::CONTROL, '\''),
        KeyBindingAction::PreviousApp => {
            KeyBinding::character(Modifiers::CONTROL | Modifiers::SHIFT, '\'')
        }
        KeyBindingAction::Cut => KeyBinding::character(CMD_OR_CONTROL, 'X'),
        KeyBindingAction::Copy => KeyBinding::character(CMD_OR_CONTROL, 'C'),
        KeyBindingAction::Paste => KeyBinding::character(CMD_OR_CONTROL, 'V'),
        KeyBindingAction::SelectAll => KeyBinding::character(CMD_OR_CONTROL, 'A'),
    }
}

fn modifier_mask() -> Modifiers {
    Modifiers::SHIFT | Modifiers::CONTROL | Modifiers::ALT | Modifiers::META
}

fn current_key_bindings_lock() -> &'static RwLock<SlateKeyBindings> {
    static CURRENT_KEY_BINDINGS: OnceLock<RwLock<SlateKeyBindings>> = OnceLock::new();
    CURRENT_KEY_BINDINGS.get_or_init(|| RwLock::new(SlateKeyBindings::default()))
}

pub(crate) fn current_key_bindings_json_value() -> serde_json::Value {
    current_key_bindings().json_value()
}

pub(crate) fn current_key_bindings() -> SlateKeyBindings {
    match current_key_bindings_lock().read() {
        Ok(bindings) => bindings.clone(),
        Err(error) => error.into_inner().clone(),
    }
}

pub(crate) fn set_current_key_bindings(bindings: SlateKeyBindings) {
    match current_key_bindings_lock().write() {
        Ok(mut current) => *current = bindings,
        Err(error) => *error.into_inner() = bindings,
    }
}

pub(crate) fn key_binding_action_for_event(event: &KeyboardEvent) -> Option<KeyBindingAction> {
    current_key_bindings().action_for_event(event)
}

pub(crate) fn key_bindings_from_settings_url(url: &Url) -> Option<SlateKeyBindings> {
    let mut bindings = current_key_bindings();
    let mut found = false;

    for action in KEY_BINDING_ACTIONS {
        let Some(value) = query_value(url, action.query_parameter()) else {
            continue;
        };
        let binding = KeyBinding::parse(&value).unwrap_or_else(|| default_key_binding(action));
        bindings.set_binding(action, binding);
        found = true;
    }

    found.then_some(bindings)
}

pub(crate) fn initialize_key_bindings_from_database(database: &SlateProfileDatabase) {
    let mut bindings = SlateKeyBindings::default();
    for action in KEY_BINDING_ACTIONS {
        let binding = load_key_binding_from_database(database, action);
        bindings.set_binding(action, binding);
    }
    set_current_key_bindings(bindings);
}

pub(crate) fn persist_key_bindings_to_database(
    database: &SlateProfileDatabase,
    bindings: &SlateKeyBindings,
) {
    for action in KEY_BINDING_ACTIONS {
        if let Err(error) = database.set_setting_text(
            action.setting_key(),
            &bindings.binding(action).setting_value(),
        ) {
            warn!(
                "failed to persist {} key binding: {error}",
                action.label().to_ascii_lowercase()
            );
        }
    }
}

fn load_key_binding_from_database(
    database: &SlateProfileDatabase,
    action: KeyBindingAction,
) -> KeyBinding {
    let stored =
        match database.ensure_setting_text(action.setting_key(), action.default_setting_value()) {
            Ok(stored) => stored,
            Err(error) => {
                warn!(
                    "failed to load {} key binding: {error}",
                    action.label().to_ascii_lowercase()
                );
                return default_key_binding(action);
            }
        };

    if legacy_default_setting_value(action).is_some_and(|legacy| stored == legacy) {
        let default = default_key_binding(action);
        if let Err(error) =
            database.set_setting_text(action.setting_key(), action.default_setting_value())
        {
            warn!(
                "failed to migrate default {} key binding: {error}",
                action.label().to_ascii_lowercase()
            );
        }
        return default;
    }

    if let Some(binding) = KeyBinding::parse(&stored) {
        return binding;
    }

    warn!(
        "resetting invalid {} key binding to default",
        action.label().to_ascii_lowercase()
    );
    let default = default_key_binding(action);
    if let Err(error) =
        database.set_setting_text(action.setting_key(), action.default_setting_value())
    {
        warn!(
            "failed to persist default {} key binding: {error}",
            action.label().to_ascii_lowercase()
        );
    }
    default
}

fn legacy_default_setting_value(action: KeyBindingAction) -> Option<&'static str> {
    match action {
        KeyBindingAction::NextTab => Some("Ctrl+PageDown"),
        KeyBindingAction::PreviousTab => Some("Ctrl+PageUp"),
        KeyBindingAction::PreviousApp => Some("Ctrl+Shift+\""),
        _ => None,
    }
}

fn query_value(url: &Url, key: &str) -> Option<String> {
    url.query_pairs()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.into_owned())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    use keyboard_types::Location;

    use super::*;

    fn unique_database_path(name: &str) -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        std::env::temp_dir().join(format!(
            "slate-key-bindings-{name}-{}-{}.db",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ))
    }

    fn key_event(key: Key, modifiers: Modifiers) -> KeyboardEvent {
        let mut event = KeyboardEvent::key_down(key, Code::Unidentified);
        event.location = Location::Standard;
        event.modifiers = modifiers;
        event
    }

    #[test]
    fn default_tab_key_bindings_match_expected_events() {
        let bindings = SlateKeyBindings::default();

        assert_eq!(
            bindings.action_for_event(&key_event(Key::Character("t".into()), CMD_OR_CONTROL)),
            Some(KeyBindingAction::NewTab)
        );
        assert_eq!(
            bindings.action_for_event(&key_event(Key::Character("w".into()), CMD_OR_CONTROL)),
            Some(KeyBindingAction::CloseTab)
        );
        assert_eq!(
            bindings.action_for_event(&key_event(Key::Named(NamedKey::Tab), Modifiers::CONTROL,)),
            Some(KeyBindingAction::NextTab)
        );
        assert_eq!(
            bindings.action_for_event(&key_event(
                Key::Named(NamedKey::Tab),
                Modifiers::CONTROL | Modifiers::SHIFT,
            )),
            Some(KeyBindingAction::PreviousTab)
        );
        assert_eq!(
            bindings.action_for_event(&key_event(Key::Character("'".into()), Modifiers::CONTROL)),
            Some(KeyBindingAction::NextApp)
        );
        assert_eq!(
            bindings.action_for_event(&key_event(
                Key::Character("\"".into()),
                Modifiers::CONTROL | Modifiers::SHIFT,
            )),
            None
        );

        let mut shifted_quote = key_event(
            Key::Character("\"".into()),
            Modifiers::CONTROL | Modifiers::SHIFT,
        );
        shifted_quote.code = keyboard_types::Code::Quote;
        assert_eq!(
            bindings.action_for_event(&shifted_quote),
            Some(KeyBindingAction::PreviousApp)
        );
        assert_eq!(
            bindings.action_for_event(&key_event(Key::Character("x".into()), CMD_OR_CONTROL)),
            Some(KeyBindingAction::Cut)
        );
        assert_eq!(
            bindings.action_for_event(&key_event(Key::Character("c".into()), CMD_OR_CONTROL)),
            Some(KeyBindingAction::Copy)
        );
        assert_eq!(
            bindings.action_for_event(&key_event(Key::Character("v".into()), CMD_OR_CONTROL)),
            Some(KeyBindingAction::Paste)
        );
        assert_eq!(
            bindings.action_for_event(&key_event(Key::Character("a".into()), CMD_OR_CONTROL)),
            Some(KeyBindingAction::SelectAll)
        );
    }

    #[test]
    fn key_binding_parser_accepts_common_shortcut_text() {
        assert_eq!(
            KeyBinding::parse("Ctrl+Shift+T").map(|binding| binding.setting_value()),
            Some("Ctrl+Shift+T".to_string())
        );
        assert_eq!(
            KeyBinding::parse("Primary+W").map(|binding| binding.key),
            Some(BindingKey::Character('W'))
        );
        assert_eq!(
            KeyBinding::parse("Ctrl+Page Down").map(|binding| binding.setting_value()),
            Some("Ctrl+PageDown".to_string())
        );
        assert_eq!(
            KeyBinding::parse("Ctrl+Tab").map(|binding| binding.setting_value()),
            Some("Ctrl+Tab".to_string())
        );
        assert_eq!(
            KeyBinding::parse("Ctrl+Shift+'").map(|binding| binding.setting_value()),
            Some("Ctrl+Shift+'".to_string())
        );
        assert!(KeyBinding::parse("Ctrl+PageDown+T").is_none());
    }

    #[test]
    fn settings_url_updates_only_provided_bindings() {
        set_current_key_bindings(SlateKeyBindings::default());
        let bindings = key_bindings_from_settings_url(
            &Url::parse("slate://settings/save?key_next_tab=Alt%2BArrowRight").unwrap(),
        )
        .expect("settings URL should include a binding update");

        assert_eq!(
            bindings.binding(KeyBindingAction::NextTab).display_label(),
            "Alt+ArrowRight"
        );
        assert_eq!(
            bindings.binding(KeyBindingAction::NewTab).display_label(),
            default_key_binding(KeyBindingAction::NewTab).display_label()
        );
    }

    #[test]
    fn settings_json_contains_defaults_and_current_values() {
        let bindings = SlateKeyBindings::default();
        let value = bindings.json_value();
        let entries = value.as_array().expect("shortcuts should be an array");

        assert_eq!(entries.len(), 10);
        assert_eq!(entries[0]["id"], "new_tab");
        assert_eq!(entries[0]["label"], "New tab");
        assert_eq!(entries[0]["default_value"], "Primary+T");
        assert!(entries.iter().any(|entry| {
            entry["id"] == "next_tab"
                && entry["query"] == "key_next_tab"
                && entry["default_value"] == "Ctrl+Tab"
        }));
        assert!(entries.iter().any(|entry| entry["id"] == "previous_tab"));
        assert!(entries.iter().any(|entry| {
            entry["id"] == "next_app"
                && entry["query"] == "key_next_app"
                && entry["default_value"] == "Ctrl+'"
        }));
        assert!(entries.iter().any(|entry| {
            entry["id"] == "previous_app"
                && entry["query"] == "key_previous_app"
                && entry["default_value"] == "Ctrl+Shift+'"
        }));
        assert!(entries.iter().any(|entry| {
            entry["id"] == "copy"
                && entry["query"] == "key_copy"
                && entry["default_value"] == "Primary+C"
        }));
        assert!(entries.iter().any(|entry| {
            entry["id"] == "select_all"
                && entry["query"] == "key_select_all"
                && entry["default_value"] == "Primary+A"
        }));
    }

    #[test]
    fn database_initialization_seeds_missing_and_resets_invalid_bindings() {
        let path = unique_database_path("defaults");
        let database = SlateProfileDatabase::open_resolved(path.clone()).unwrap();
        database
            .set_setting_text(KeyBindingAction::CloseTab.setting_key(), "not a shortcut")
            .unwrap();

        initialize_key_bindings_from_database(&database);

        assert_eq!(
            database
                .get_setting_text(KeyBindingAction::NewTab.setting_key())
                .unwrap()
                .as_deref(),
            Some(KeyBindingAction::NewTab.default_setting_value())
        );
        assert_eq!(
            database
                .get_setting_text(KeyBindingAction::CloseTab.setting_key())
                .unwrap()
                .as_deref(),
            Some(KeyBindingAction::CloseTab.default_setting_value())
        );

        drop(database);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn database_initialization_migrates_old_shortcut_defaults() {
        let path = unique_database_path("legacy-tab-defaults");
        let database = SlateProfileDatabase::open_resolved(path.clone()).unwrap();
        database
            .set_setting_text(KeyBindingAction::NextTab.setting_key(), "Ctrl+PageDown")
            .unwrap();
        database
            .set_setting_text(KeyBindingAction::PreviousTab.setting_key(), "Ctrl+PageUp")
            .unwrap();
        database
            .set_setting_text(KeyBindingAction::PreviousApp.setting_key(), "Ctrl+Shift+\"")
            .unwrap();

        initialize_key_bindings_from_database(&database);

        assert_eq!(
            database
                .get_setting_text(KeyBindingAction::NextTab.setting_key())
                .unwrap()
                .as_deref(),
            Some("Ctrl+Tab")
        );
        assert_eq!(
            database
                .get_setting_text(KeyBindingAction::PreviousTab.setting_key())
                .unwrap()
                .as_deref(),
            Some("Ctrl+Shift+Tab")
        );
        assert_eq!(
            database
                .get_setting_text(KeyBindingAction::PreviousApp.setting_key())
                .unwrap()
                .as_deref(),
            Some("Ctrl+Shift+'")
        );

        drop(database);
        let _ = std::fs::remove_file(path);
    }
}
