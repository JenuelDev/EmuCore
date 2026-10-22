//! Serializes client launch requests without starting or inspecting an engine.
#![forbid(unsafe_code)]

use crate::{
    Error, ErrorKind, Result,
    game_library::{GameEntry, Installation, RuntimeProfile},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LaunchTicket {
    generation: u64,
    entry_id: String,
    profile: RuntimeProfile,
    reuse_instance: bool,
}

impl LaunchTicket {
    pub fn entry_id(&self) -> &str {
        &self.entry_id
    }
    pub fn profile(&self) -> &RuntimeProfile {
        &self.profile
    }
    pub fn reuse_instance(&self) -> bool {
        self.reuse_instance
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LibraryLaunchState {
    Idle,
    Starting { entry_id: String },
    Running { entry_id: String },
    Stopping,
}

#[derive(Default)]
pub struct LaunchGate {
    generation: u64,
    pending: Option<LaunchTicket>,
    running: Option<LaunchTicket>,
    stopping: bool,
}

impl LaunchGate {
    /// Call after resolving an entry/profile from a validated library snapshot.
    pub fn request(&mut self, entry: &GameEntry, profile: &RuntimeProfile) -> Result<LaunchTicket> {
        if !matches!(entry.installation, Installation::Installed { verified_at_unix_ms } if verified_at_unix_ms > 0)
        {
            return Err(usage(
                "the selected entry has no cached installation verification",
            ));
        }
        if entry.runtime_profile_id != profile.id {
            return Err(usage("selected entry/profile mismatch"));
        }
        if self.stopping || self.pending.is_some() {
            return Err(usage("wait for the current launch or stop request"));
        }
        if let Some(running) = &self.running
            && running.profile != *profile
        {
            return Err(usage(
                "stop the current emulator before changing its runtime profile",
            ));
        }
        self.generation = self
            .generation
            .checked_add(1)
            .ok_or_else(|| usage("launch request counter exhausted"))?;
        let ticket = LaunchTicket {
            generation: self.generation,
            entry_id: entry.id.clone(),
            profile: profile.clone(),
            reuse_instance: self.running.is_some(),
        };
        self.pending = Some(ticket.clone());
        Ok(ticket)
    }

    /// False means a stale completion, including one invalidated by cancellation.
    pub fn finish(&mut self, ticket: &LaunchTicket, succeeded: bool) -> bool {
        if self.pending.as_ref() != Some(ticket) || self.stopping {
            return false;
        }
        self.pending = None;
        self.running = succeeded.then(|| ticket.clone());
        true
    }

    /// Returns whether a pending launch needs its cancellation token signalled.
    pub fn begin_stop(&mut self) -> bool {
        let cancel_pending = self.pending.take().is_some();
        self.stopping = true;
        cancel_pending
    }

    pub fn finish_stop(&mut self) {
        self.pending = None;
        self.running = None;
        self.stopping = false;
    }

    pub fn state(&self) -> LibraryLaunchState {
        if self.stopping {
            LibraryLaunchState::Stopping
        } else if let Some(ticket) = &self.pending {
            LibraryLaunchState::Starting {
                entry_id: ticket.entry_id.clone(),
            }
        } else if let Some(ticket) = &self.running {
            LibraryLaunchState::Running {
                entry_id: ticket.entry_id.clone(),
            }
        } else {
            LibraryLaunchState::Idle
        }
    }

    pub(crate) fn pending_is(&self, ticket: &LaunchTicket) -> bool {
        self.pending.as_ref() == Some(ticket)
    }
}

fn usage(message: &str) -> Error {
    Error::new(ErrorKind::Usage, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_library::RuntimeEngine;

    fn entry(id: &str) -> GameEntry {
        GameEntry {
            id: id.into(),
            title: id.into(),
            component: "dev.game/.Main".into(),
            runtime_profile_id: "reference".into(),
            installation: Installation::Installed {
                verified_at_unix_ms: 1,
            },
            icon_path: None,
            fixture: None,
        }
    }
    fn profile() -> RuntimeProfile {
        RuntimeProfile {
            id: "reference".into(),
            name: "External reference".into(),
            engine: RuntimeEngine::AndroidSdkReference,
            sdk_root: "not-accessed-sdk".into(),
        }
    }

    #[test]
    fn starts_only_on_request_and_rejects_duplicate_pending_launches() {
        let mut gate = LaunchGate::default();
        assert_eq!(gate.state(), LibraryLaunchState::Idle);
        let ticket = gate.request(&entry("one"), &profile()).unwrap();
        assert!(!ticket.reuse_instance());
        assert!(gate.request(&entry("one"), &profile()).is_err());
        assert!(gate.finish(&ticket, true));
        // A later click rechecks the same app through the existing instance. It
        // must not assume the user has kept that activity open in the guest.
        let reopened = gate.request(&entry("one"), &profile()).unwrap();
        assert!(reopened.reuse_instance());
        assert_ne!(reopened, ticket);
    }

    #[test]
    fn reuses_only_the_same_complete_runtime_profile() {
        let mut gate = LaunchGate::default();
        let ticket = gate.request(&entry("one"), &profile()).unwrap();
        gate.finish(&ticket, true);
        let mut changed = profile();
        changed.sdk_root = "different-sdk".into();
        assert!(gate.request(&entry("two"), &changed).is_err());
        let next = gate.request(&entry("two"), &profile()).unwrap();
        assert!(next.reuse_instance());
        assert!(gate.finish(&next, true));
        assert_eq!(
            gate.state(),
            LibraryLaunchState::Running {
                entry_id: "two".into()
            }
        );
    }

    #[test]
    fn stale_completion_cannot_revive_a_cancelled_or_newer_launch() {
        let mut gate = LaunchGate::default();
        let old = gate.request(&entry("one"), &profile()).unwrap();
        assert!(gate.begin_stop());
        assert!(!gate.finish(&old, true));
        assert!(gate.request(&entry("two"), &profile()).is_err());
        gate.finish_stop();
        let current = gate.request(&entry("two"), &profile()).unwrap();
        assert!(!gate.finish(&old, true));
        assert!(gate.finish(&current, true));
        assert_eq!(
            gate.state(),
            LibraryLaunchState::Running {
                entry_id: "two".into()
            }
        );
    }

    #[test]
    fn failure_releases_the_gate_for_an_explicit_retry() {
        let mut gate = LaunchGate::default();
        let failed = gate.request(&entry("one"), &profile()).unwrap();
        assert!(gate.finish(&failed, false));
        assert_eq!(gate.state(), LibraryLaunchState::Idle);
        let retried = gate.request(&entry("one"), &profile()).unwrap();
        assert_ne!(failed, retried);
    }

    #[test]
    fn unavailable_entries_and_wrong_profiles_never_get_a_launch_ticket() {
        let mut gate = LaunchGate::default();
        let mut missing = entry("one");
        missing.installation = Installation::NotInstalled;
        assert!(gate.request(&missing, &profile()).is_err());
        let mut wrong = profile();
        wrong.id = "other".into();
        assert!(gate.request(&entry("one"), &wrong).is_err());
        assert_eq!(gate.state(), LibraryLaunchState::Idle);
    }
}
