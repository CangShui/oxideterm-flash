use super::*;

impl WorkspaceApp {
    pub(in crate::workspace) fn open_onboarding_from_palette(&mut self, cx: &mut Context<Self>) {
        let disclaimer_accepted = self.onboarding.disclaimer_accepted
            || self
                .settings_store
                .settings()
                .onboarding_disclaimer_accepted
            || self.settings_store.settings().onboarding_completed;
        self.edit_settings(
            move |settings| {
                settings.onboarding_completed = false;
                // Persist the implied acceptance when migrating a completed legacy flow.
                settings.onboarding_disclaimer_accepted = disclaimer_accepted;
            },
            cx,
        );
        self.onboarding
            .reset_for_open(self.settings_store.settings());
        cx.notify();
    }

    pub(in crate::workspace) fn complete_onboarding(&mut self, cx: &mut Context<Self>) {
        // The disclaimer page no longer exists; completing welcome implies its
        // acceptance so downstream consumers keep seeing a consistent state.
        self.edit_settings(
            move |settings| {
                settings.onboarding_completed = true;
                settings.onboarding_disclaimer_accepted = true;
            },
            cx,
        );
        self.onboarding.open = false;
        cx.notify();
    }

    pub(in crate::workspace) fn close_onboarding_if_allowed(&mut self, cx: &mut Context<Self>) {
        if self.onboarding.disclaimer_accepted {
            self.complete_onboarding(cx);
        }
    }

    pub(in crate::workspace) fn handle_onboarding_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.onboarding.open {
            return false;
        }
        match event.keystroke.key.as_str() {
            "escape" => self.close_onboarding_if_allowed(cx),
            // Enter mirrors the footer start button because welcome is the only step.
            "enter" => self.complete_onboarding(cx),
            _ => return false,
        }
        true
    }
}
