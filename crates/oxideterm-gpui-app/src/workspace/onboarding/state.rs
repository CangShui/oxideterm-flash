use super::*;

#[allow(dead_code)]
pub(in crate::workspace) const ONBOARDING_TOTAL_STEPS: usize = 1;
pub(in crate::workspace) const ONBOARDING_WIDTH: f32 = 800.0; // Tauri DialogContent sm:max-w-[800px].
pub(in crate::workspace) const ONBOARDING_MAX_HEIGHT: f32 = 720.0;
pub(in crate::workspace) const ONBOARDING_ICON_SIZE: f32 = 16.0;
pub(in crate::workspace) const ONBOARDING_ACCENT_SUBTLE_ALPHA: u32 = 0x0d; // Tauri accent/5.
pub(in crate::workspace) const ONBOARDING_ACCENT_BORDER_ALPHA: u32 = 0x33; // Tauri accent/20.
pub(in crate::workspace) const ONBOARDING_CARD_ALPHA: u32 = 0xcc; // Browser panels sit over the dialog backdrop but stay readable.
pub(in crate::workspace) const ONBOARDING_DISABLED_OPACITY: f32 = 0.45;

pub(in crate::workspace) const ONBOARDING_LANGUAGES: [(Language, &str); 2] = [
    (Language::En, "English"),
    (Language::ZhCn, "简体中文"),
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum OnboardingStep {
    Welcome,
}

impl OnboardingStep {
    pub(in crate::workspace) fn from_index(_index: usize) -> Self {
        // Welcome is the only step left, so every index resolves onto it.
        Self::Welcome
    }
}

#[derive(Clone)]
pub(in crate::workspace) struct OnboardingState {
    pub(in crate::workspace) open: bool,
    pub(in crate::workspace) step: usize,
    pub(in crate::workspace) disclaimer_accepted: bool,
}

impl OnboardingState {
    pub(in crate::workspace) fn from_settings(settings: &PersistedSettings) -> Self {
        Self {
            open: !settings.onboarding_completed,
            step: 0,
            disclaimer_accepted: disclaimer_accepted_from_settings(settings),
        }
    }

    pub(in crate::workspace) fn reset_for_open(&mut self, settings: &PersistedSettings) {
        self.open = true;
        self.step = 0;
        self.disclaimer_accepted = disclaimer_accepted_from_settings(settings);
    }
}

fn disclaimer_accepted_from_settings(settings: &PersistedSettings) -> bool {
    // Completed legacy onboarding flows necessarily passed the disclaimer step.
    settings.onboarding_disclaimer_accepted || settings.onboarding_completed
}

#[cfg(test)]
mod tests {
    use super::disclaimer_accepted_from_settings;
    use oxideterm_settings::PersistedSettings;

    #[test]
    fn persisted_and_legacy_onboarding_accept_the_disclaimer() {
        for mut settings in [
            PersistedSettings {
                onboarding_disclaimer_accepted: true,
                ..PersistedSettings::default()
            },
            PersistedSettings {
                onboarding_completed: true,
                ..PersistedSettings::default()
            },
        ] {
            assert!(disclaimer_accepted_from_settings(&settings));
            settings.onboarding_disclaimer_accepted = false;
            settings.onboarding_completed = false;
            assert!(!disclaimer_accepted_from_settings(&settings));
        }
    }
}
