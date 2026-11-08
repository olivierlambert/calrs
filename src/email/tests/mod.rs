//! Tests for the email module. Shared fixtures live here; each child
//! module covers one area and reaches them, and every item of `email`,
//! through `use super::*`.

use lettre::Address;

use super::*;

mod guest_footer;
mod messages;
mod render;
mod transport;

// The crate-wide lock, not a module-local one: web tests that send real
// mail through `load_smtp_config` must not see a half-set
// `CALRS_SMTP_*` block from these tests running on another thread.
struct SmtpEnvGuard {
    _lock: tokio::sync::MutexGuard<'static, ()>,
    old_values: Vec<(&'static str, Option<String>)>,
}

impl SmtpEnvGuard {
    fn new() -> Self {
        let lock = crate::test_support::ENV_LOCK.blocking_lock();
        let old_values = SMTP_ENV_VARS
            .iter()
            .map(|name| (*name, std::env::var(name).ok()))
            .collect();
        for name in SMTP_ENV_VARS {
            std::env::remove_var(name);
        }
        Self {
            _lock: lock,
            old_values,
        }
    }
}

impl Drop for SmtpEnvGuard {
    fn drop(&mut self) {
        for (name, value) in &self.old_values {
            match value {
                Some(value) => std::env::set_var(name, value),
                None => std::env::remove_var(name),
            }
        }
    }
}
