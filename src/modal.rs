//! Modal dialogs for profile CRUD.
//!
//! Pure state and validation logic — no rendering — so it is unit-testable.
//! Rendering lives in [`crate::ui`]; key handling in [`crate::app`].

use zapret2_core::daemon::DaemonManager;
use zapret2_core::profile::{Profile, ProfileManager};

/// Number of editable fields in the profile form.
pub const FIELD_COUNT: usize = 5;

/// Which modal, if any, is active.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Modal {
    /// No modal; normal tab interaction.
    #[default]
    None,
    /// Create or edit a profile.
    Form(ProfileForm),
    /// Confirm deletion of the named profile.
    DeleteConfirm { name: String },
}

impl Modal {
    pub fn is_open(&self) -> bool {
        !matches!(self, Modal::None)
    }
}

/// Editable profile form state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileForm {
    /// `None` when creating; `Some(original_name)` when editing an existing
    /// profile (so a rename can remove the old file).
    pub editing: Option<String>,
    pub name: String,
    pub description: String,
    pub strategy: String,
    pub nfqws_opts: String,
    /// Hostlists as a single comma/whitespace-separated string for editing.
    pub hostlists: String,
    /// Focused field index, `0..FIELD_COUNT`.
    pub focus: usize,
    /// Validation feedback for the last submit attempt.
    pub error: Option<String>,
}

impl ProfileForm {
    /// An empty form for creating a new profile.
    pub fn create() -> Self {
        Self {
            editing: None,
            name: String::new(),
            description: String::new(),
            strategy: String::new(),
            nfqws_opts: String::new(),
            hostlists: String::new(),
            focus: 0,
            error: None,
        }
    }

    /// A form pre-filled from an existing profile for editing.
    pub fn edit(profile: &Profile) -> Self {
        Self {
            editing: Some(profile.name.clone()),
            name: profile.name.clone(),
            description: profile.description.clone(),
            strategy: profile.strategy.clone(),
            nfqws_opts: profile.nfqws_opts.clone(),
            hostlists: profile.hostlists.join(", "),
            focus: 0,
            error: None,
        }
    }

    /// Whether this form edits an existing profile.
    pub fn is_edit(&self) -> bool {
        self.editing.is_some()
    }

    /// Title for the modal frame.
    pub fn title(&self) -> &'static str {
        if self.is_edit() {
            "Edit profile"
        } else {
            "Create profile"
        }
    }

    /// Labels for each field, in focus order.
    pub const fn field_labels() -> [&'static str; FIELD_COUNT] {
        ["Name", "Description", "Strategy", "nfqws opts", "Hostlists"]
    }

    /// The value of the field at `index`, for rendering.
    pub fn field_value(&self, index: usize) -> &str {
        match index {
            0 => &self.name,
            1 => &self.description,
            2 => &self.strategy,
            3 => &self.nfqws_opts,
            4 => &self.hostlists,
            _ => "",
        }
    }

    fn focused_value_mut(&mut self) -> &mut String {
        match self.focus {
            0 => &mut self.name,
            1 => &mut self.description,
            2 => &mut self.strategy,
            3 => &mut self.nfqws_opts,
            _ => &mut self.hostlists,
        }
    }

    /// Append a typed character to the focused field.
    pub fn input(&mut self, c: char) {
        self.focused_value_mut().push(c);
    }

    /// Delete the last character of the focused field.
    pub fn backspace(&mut self) {
        self.focused_value_mut().pop();
    }

    /// Move focus to the next field (wraps).
    pub fn focus_next(&mut self) {
        self.focus = (self.focus + 1) % FIELD_COUNT;
    }

    /// Move focus to the previous field (wraps).
    pub fn focus_prev(&mut self) {
        self.focus = (self.focus + FIELD_COUNT - 1) % FIELD_COUNT;
    }

    /// Parse the hostlists string into a clean list (split on commas and
    /// whitespace, trimmed, empties dropped).
    pub fn parse_hostlists(&self) -> Vec<String> {
        self.hostlists
            .split([',', ' ', '\t', '\n'])
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect()
    }

    /// Build a [`Profile`] from the current field values.
    pub fn to_profile(&self) -> Profile {
        Profile {
            name: self.name.trim().to_string(),
            description: self.description.trim().to_string(),
            strategy: self.strategy.trim().to_string(),
            hostlists: self.parse_hostlists(),
            nfqws_opts: self.nfqws_opts.trim().to_string(),
        }
    }

    /// Validate the form. Returns `Ok` with the resulting profile, or `Err`
    /// with a human-readable message. Runs the same checks that gate any
    /// privileged write, so invalid input never reaches the filesystem.
    pub fn validate(&self) -> Result<Profile, String> {
        let profile = self.to_profile();
        ProfileManager::validate_name(&profile.name).map_err(|e| e.to_string())?;
        DaemonManager::validate_opts(&profile.nfqws_opts).map_err(|e| e.to_string())?;
        Ok(profile)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Profile {
        Profile {
            name: "yt".to_string(),
            description: "YouTube".to_string(),
            strategy: "split".to_string(),
            hostlists: vec!["youtube.txt".to_string(), "google.txt".to_string()],
            nfqws_opts: "--qnum=300".to_string(),
        }
    }

    #[test]
    fn edit_form_roundtrips_profile_fields() {
        let form = ProfileForm::edit(&sample());
        assert!(form.is_edit());
        assert_eq!(form.editing.as_deref(), Some("yt"));
        assert_eq!(form.hostlists, "youtube.txt, google.txt");

        let p = form.to_profile();
        assert_eq!(p.name, "yt");
        assert_eq!(p.hostlists, vec!["youtube.txt", "google.txt"]);
        assert_eq!(p.nfqws_opts, "--qnum=300");
    }

    #[test]
    fn focus_wraps_both_directions() {
        let mut form = ProfileForm::create();
        assert_eq!(form.focus, 0);
        form.focus_prev();
        assert_eq!(form.focus, FIELD_COUNT - 1);
        form.focus_next();
        assert_eq!(form.focus, 0);
    }

    #[test]
    fn input_and_backspace_target_focused_field() {
        let mut form = ProfileForm::create();
        for c in "abc".chars() {
            form.input(c);
        }
        assert_eq!(form.name, "abc");
        form.backspace();
        assert_eq!(form.name, "ab");

        form.focus_next(); // description
        form.input('x');
        assert_eq!(form.description, "x");
        assert_eq!(form.name, "ab");
    }

    #[test]
    fn parse_hostlists_splits_and_trims() {
        let mut form = ProfileForm::create();
        form.hostlists = "  a.txt , b.txt   c.txt,,".to_string();
        assert_eq!(form.parse_hostlists(), vec!["a.txt", "b.txt", "c.txt"]);
    }

    #[test]
    fn validate_rejects_bad_name_before_write() {
        let mut form = ProfileForm::create();
        form.name = "../evil".to_string();
        form.nfqws_opts = "--qnum=200".to_string();
        assert!(form.validate().is_err());
    }

    #[test]
    fn validate_rejects_forbidden_opts_before_write() {
        let mut form = ProfileForm::create();
        form.name = "ok".to_string();
        form.nfqws_opts = "--rm -rf".to_string();
        assert!(form.validate().is_err());
    }

    #[test]
    fn validate_accepts_clean_form() {
        let mut form = ProfileForm::create();
        form.name = "myprofile".to_string();
        form.nfqws_opts = "--qnum=200 --dpi-desync".to_string();
        form.hostlists = "a.txt, b.txt".to_string();
        let p = form.validate().expect("should be valid");
        assert_eq!(p.name, "myprofile");
        assert_eq!(p.hostlists, vec!["a.txt", "b.txt"]);
    }
}
