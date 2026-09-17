//! Security settings: the default sandbox level for new chats.
//!
//! Stored in its own `security-defaults.json` file rather than in
//! `UiSettings`, because it only applies to chats created after it changes.
//! Existing chats keep their own saved level.
//!
//! Follows the shortcuts and notifications pages: the page keeps a working
//! copy, each choice emits [`SecurityEvent::Changed`], and the shell saves it.

use std::io;
use std::path::{Path, PathBuf};

use gpui::{Context, EventEmitter, SharedString, Window, div, prelude::*, px};
use komet_proto::SandboxLevel;
use serde::{Deserialize, Serialize};

use crate::icons;
use crate::settings::widgets;
use crate::theme::Theme;

const FILE_NAME: &str = "security-defaults.json";

/// Saved security settings. `default_access` is the sandbox level new chats
/// start at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SecurityDefaults {
    pub default_access: SandboxLevel,
}

impl Default for SecurityDefaults {
    fn default() -> Self {
        Self {
            default_access: SandboxLevel::WorkspaceWrite,
        }
    }
}

impl SecurityDefaults {
    /// Load `{data_dir}/security-defaults.json`, falling back to the defaults
    /// when the file is missing or unreadable.
    pub fn load(data_dir: &Path) -> Self {
        match std::fs::read_to_string(Self::path(data_dir)) {
            Ok(text) => match serde_json::from_str::<SecurityDefaults>(&text) {
                Ok(defaults) => defaults,
                Err(err) => {
                    tracing::warn!(error = %err, "security-defaults corrupt; using defaults");
                    Self::default()
                }
            },
            Err(_) => Self::default(),
        }
    }

    /// Write to a temporary file and rename it, so a crash cannot leave a
    /// partially written file.
    pub fn save(&self, data_dir: &Path) -> io::Result<()> {
        std::fs::create_dir_all(data_dir)?;
        let path = Self::path(data_dir);
        let tmp = path.with_extension("json.tmp");
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, &path)
    }

    pub fn path(data_dir: &Path) -> PathBuf {
        data_dir.join(FILE_NAME)
    }
}

/// Use the saved default as the level for new chats. Existing chats keep
/// their own saved level.
pub fn seed_new_chat_access(state: &mut crate::state::AppState, defaults: &SecurityDefaults) {
    state.new_chat_access = defaults.default_access;
    state.sync_access_mode();
}

/// Apply a choice made on the Security page: save it to
/// `security-defaults.json` and use it for new chats immediately. If an
/// existing chat is open, that chat keeps its own level. Returns the save
/// result so the caller can report a failure.
pub fn apply_security_pick(
    defaults: &mut SecurityDefaults,
    state: &mut crate::state::AppState,
    data_dir: &Path,
    default_access: SandboxLevel,
) -> io::Result<()> {
    defaults.default_access = default_access;
    let result = defaults.save(data_dir);
    state.new_chat_access = default_access;
    state.sync_access_mode();
    result
}

/// A new default chosen on the Security page.
#[derive(Debug, Clone, Copy)]
pub enum SecurityEvent {
    Changed { default_access: SandboxLevel },
}

pub struct SecurityPage {
    default_access: SandboxLevel,
}

impl EventEmitter<SecurityEvent> for SecurityPage {}

impl SecurityPage {
    pub fn new(default_access: SandboxLevel, _cx: &mut Context<Self>) -> Self {
        Self { default_access }
    }

    /// The three levels with the composer's wording: level, title, description
    /// and icon.
    fn rows() -> [(SandboxLevel, &'static str, &'static str, &'static str); 3] {
        [
            (
                SandboxLevel::ReadOnly,
                "Read only",
                "Agents can read files in the workspace but cannot modify anything \
                 or run commands that write.",
                icons::EYE,
            ),
            (
                SandboxLevel::WorkspaceWrite,
                "Sandboxed",
                "Agents can read and write inside the workspace; access to the rest \
                 of the system is blocked. The default.",
                icons::FOLDER_WITH_FILES,
            ),
            (
                SandboxLevel::DangerFullAccess,
                "Full access",
                "Agents can read and write anywhere on this device and run commands \
                 without sandbox restrictions. Use only when you trust the run.",
                icons::DANGER_TRIANGLE,
            ),
        ]
    }
}

impl Render for SecurityPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx).clone();
        let selected = self.default_access;

        let mut card = widgets::section_card(&theme);
        let rows = Self::rows();
        for (idx, (level, title, description, icon)) in rows.iter().enumerate() {
            // Copied because the click listener cannot borrow `rows`.
            let level = *level;
            let is_selected = level == selected;
            let row_id = match level {
                SandboxLevel::ReadOnly => "security-row-read-only",
                SandboxLevel::WorkspaceWrite => "security-row-sandboxed",
                SandboxLevel::DangerFullAccess => "security-row-full-access",
            };
            card = card.child(
                widgets::card_row(&theme, idx == 0)
                    .id(row_id)
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.default_access = level;
                        cx.emit(SecurityEvent::Changed {
                            default_access: level,
                        });
                        cx.notify();
                    }))
                    .child(widgets::row_tile(&theme, icon))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(widgets::row_title(&theme, *title))
                            .child(widgets::meta_line(
                                &theme,
                                vec![
                                    div()
                                        .child(SharedString::from(*description))
                                        .into_any_element(),
                                ],
                            )),
                    )
                    .children(is_selected.then(|| widgets::badge_active(&theme, "Default"))),
            );
        }

        let full_access_warning = widgets::warning_strip(
            &theme,
            "Full access disables the sandbox: the agent can change any file and run \
             any command on this device. This default only applies to new chats; \
             existing chats keep their own level.",
        );

        div()
            .id("security-page")
            .size_full()
            .overflow_y_scroll()
            .child(
                widgets::page_column()
                    .child(widgets::page_header(&theme, "Security", None))
                    .child(
                        widgets::page_subtitle(
                            &theme,
                            "The default access level for new chats. Changing it never \
                             rewrites the configuration of chats that already exist.",
                        )
                        .max_w(px(512.0))
                        .line_height(px(20.0)),
                    )
                    .child(card)
                    .when(selected == SandboxLevel::DangerFullAccess, |el| {
                        el.child(full_access_warning)
                    }),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;
    use chrono::Utc;
    use komet_proto::{Chat, ChatConfig, HarnessId};

    /// An existing chat saved at ReadOnly. Changing the default must not
    /// change it (scenario 4).
    fn existing_chat() -> Chat {
        Chat {
            id: "chat-1".into(),
            device_id: "d".into(),
            title: None,
            archived: false,
            cwd: None,
            branch: None,
            checkout_id: None,
            config: Some(ChatConfig {
                harness: HarnessId::ClaudeCode,
                model: None,
                reasoning: None,
                model_options: Default::default(),
                sandbox: SandboxLevel::ReadOnly,
                mcp_server_ids: Vec::new(),
            }),
            last_message_preview: None,
            last_message_at: None,
            created_at: Utc::now(),
            harness_session_id: None,
            harness_session_cwd: None,
            space_id: None,
            last_seen_at: None,
            room_gen: None,
        }
    }

    // Scenario 1: without a saved file, new chats start at WorkspaceWrite.
    #[test]
    fn default_is_workspace_write() {
        assert_eq!(
            SecurityDefaults::default().default_access,
            SandboxLevel::WorkspaceWrite
        );
        let mut state = AppState::new();
        seed_new_chat_access(
            &mut state,
            &SecurityDefaults::load(Path::new("/nonexistent")),
        );
        assert_eq!(state.access_mode, SandboxLevel::WorkspaceWrite);
    }

    // Scenario 1: a saved choice is loaded again after a restart.
    #[test]
    fn picked_level_survives_restart_and_seeds_new_chat() {
        let dir = tempfile::tempdir().unwrap();
        let mut defaults = SecurityDefaults::default();
        let mut state = AppState::new();
        apply_security_pick(
            &mut defaults,
            &mut state,
            dir.path(),
            SandboxLevel::ReadOnly,
        )
        .unwrap();
        // Simulate a restart: load the saved file into a new app state.
        let reloaded = SecurityDefaults::load(dir.path());
        let mut restarted = AppState::new();
        seed_new_chat_access(&mut restarted, &reloaded);
        assert_eq!(restarted.access_mode, SandboxLevel::ReadOnly);
    }

    // Scenario 2: a missing or malformed file falls back to the defaults.
    // Malformed JSON also logs a warning.
    #[test]
    fn missing_and_corrupt_files_yield_safe_defaults() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            SecurityDefaults::load(dir.path()),
            SecurityDefaults::default()
        );
        std::fs::write(SecurityDefaults::path(dir.path()), "{nope").unwrap();
        assert_eq!(
            SecurityDefaults::load(dir.path()),
            SecurityDefaults::default()
        );
    }

    #[test]
    fn camel_case_keys_and_unknown_fields_are_tolerant() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            SecurityDefaults::path(dir.path()),
            r#"{"defaultAccess":"danger-full-access"}"#,
        )
        .unwrap();
        assert_eq!(
            SecurityDefaults::load(dir.path()).default_access,
            SandboxLevel::DangerFullAccess
        );
        // A file from a future version with extra keys still loads.
        std::fs::write(
            SecurityDefaults::path(dir.path()),
            r#"{"defaultAccess":"read-only","futureKey":true}"#,
        )
        .unwrap();
        assert_eq!(
            SecurityDefaults::load(dir.path()).default_access,
            SandboxLevel::ReadOnly
        );
    }

    #[test]
    fn missing_key_falls_back_to_field_default() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(SecurityDefaults::path(dir.path()), "{}").unwrap();
        assert_eq!(
            SecurityDefaults::load(dir.path()).default_access,
            SandboxLevel::WorkspaceWrite
        );
    }

    // Scenario 2: saving replaces the file in one step and leaves no
    // temporary file behind.
    #[test]
    fn save_is_atomic_via_temp_file() {
        let dir = tempfile::tempdir().unwrap();
        SecurityDefaults {
            default_access: SandboxLevel::DangerFullAccess,
        }
        .save(dir.path())
        .unwrap();
        let path = SecurityDefaults::path(dir.path());
        assert!(path.is_file());
        assert!(!path.with_extension("json.tmp").exists());
        // Overwriting an existing file leaves no temporary file either.
        SecurityDefaults::default().save(dir.path()).unwrap();
        assert!(!path.with_extension("json.tmp").exists());
        assert_eq!(
            SecurityDefaults::load(dir.path()),
            SecurityDefaults::default()
        );
    }

    // Scenario 5: on the new-chat screen, a new default applies without a
    // restart.
    #[test]
    fn pick_applies_live_on_the_new_chat_canvas() {
        let dir = tempfile::tempdir().unwrap();
        let mut defaults = SecurityDefaults::default();
        let mut state = AppState::new();
        state.apply_chats(vec![existing_chat()]);

        apply_security_pick(
            &mut defaults,
            &mut state,
            dir.path(),
            SandboxLevel::DangerFullAccess,
        )
        .unwrap();

        assert_eq!(state.new_chat_access, SandboxLevel::DangerFullAccess);
        assert_eq!(state.access_mode, SandboxLevel::DangerFullAccess);
    }

    // Scenarios 4 and 7: while an existing chat is open, setting the default
    // to Full access does not change that chat's level or its saved row. The
    // new default applies once the new-chat screen is shown.
    #[test]
    fn pick_never_broadens_the_selected_existing_chat() {
        let dir = tempfile::tempdir().unwrap();
        let mut defaults = SecurityDefaults::default();
        let mut state = AppState::new();
        let chat = existing_chat();
        state.apply_chats(vec![chat.clone()]);
        state.selected_chat = Some(chat.id.clone());
        state.sync_access_mode();
        assert_eq!(state.access_mode, SandboxLevel::ReadOnly);

        apply_security_pick(
            &mut defaults,
            &mut state,
            dir.path(),
            SandboxLevel::DangerFullAccess,
        )
        .unwrap();

        // The next message still uses the chat's own level.
        assert_eq!(state.access_mode, SandboxLevel::ReadOnly);
        assert_eq!(state.chats, vec![chat]);
        // On the new-chat screen the new default applies.
        state.selected_chat = None;
        state.sync_access_mode();
        assert_eq!(state.access_mode, SandboxLevel::DangerFullAccess);
    }

    // A failed save is returned to the caller, not reported as success.
    #[test]
    fn pick_failure_is_surfaced_not_silent() {
        let dir = tempfile::tempdir().unwrap();
        // A file in place of the data directory makes the save fail.
        let blocker = dir.path().join("blocker");
        std::fs::write(&blocker, "not a directory").unwrap();

        let mut defaults = SecurityDefaults::default();
        let mut state = AppState::new();
        let result = apply_security_pick(
            &mut defaults,
            &mut state,
            &blocker.join("nested"),
            SandboxLevel::ReadOnly,
        );
        assert!(result.is_err(), "save failure must not read as success");
        // The new default still applies for this session; the saved file keeps
        // its previous value and the caller logs the error.
        assert_eq!(state.access_mode, SandboxLevel::ReadOnly);
    }

    #[test]
    fn persisted_level_seeds_new_chat_access_mode() {
        let mut state = AppState::new();
        assert_eq!(state.access_mode, SandboxLevel::WorkspaceWrite);
        seed_new_chat_access(
            &mut state,
            &SecurityDefaults {
                default_access: SandboxLevel::ReadOnly,
            },
        );
        assert_eq!(state.access_mode, SandboxLevel::ReadOnly);
    }
}
