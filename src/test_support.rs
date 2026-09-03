//! Helpers for tests that touch process-global state.
//!
//! Environment variables belong to the process, and Rust runs tests in
//! parallel threads inside one process. So a test that sets `CALRS_*` races
//! every test that reads it, including tests in other modules. A lock private
//! to one module is not enough: `commands::config` had exactly that, and
//! `config_general_set_and_clear` still failed intermittently because
//! `caldav`'s allowlist test was setting the same variable from another
//! module. Everything that touches this environment now shares one lock.

/// Serialises tests that read or write the runtime-settings environment.
/// Async-aware, so a guard can be held across an `.await`.
pub(crate) static ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// The runtime-settings variables. Anything added here is also cleared and
/// restored by [`EnvGuard`].
const VARS: [&str; 2] = ["CALRS_BASE_URL", "CALRS_ALLOW_PRIVATE_HOSTS"];

/// Clears the runtime-settings env vars for the duration of a test and puts
/// them back afterwards, so one test cannot leak an override into the next.
/// Hold [`ENV_LOCK`] for as long as you hold this.
pub(crate) struct EnvGuard {
    old: Vec<(&'static str, Option<String>)>,
}

impl EnvGuard {
    pub(crate) fn new() -> Self {
        let old = VARS
            .iter()
            .map(|n| (*n, std::env::var(n).ok()))
            .collect::<Vec<_>>();
        for n in VARS {
            std::env::remove_var(n);
        }
        Self { old }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (n, v) in &self.old {
            match v {
                Some(v) => std::env::set_var(n, v),
                None => std::env::remove_var(n),
            }
        }
    }
}
