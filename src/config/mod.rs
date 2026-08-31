//! Connection profiles and their on-disk representation.
//!
//! Persistence is independent from the TUI: the application can load a
//! [`Profiles`] value once, mutate it, and persist it atomically through
//! [`ConfigStore`]. Key files live below the same directory and are addressed
//! by relative paths in the JSON document so one key can be shared by many
//! profiles.

mod keys;
mod model;
mod store;

pub use keys::{KeyError, KeyPair, KeyResult, KeyStore};
pub use model::{
    AuthMethod, CURRENT_SCHEMA_VERSION, Profile, Profiles, ValidationError,
    profile_name_is_path_safe, profile_names_equal,
};
pub use store::{ConfigError, ConfigStore, Result};

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static UNIQUE_SUFFIX_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub(super) fn unique_suffix() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    unique_suffix_at(nanos)
}

fn unique_suffix_at(nanos: u128) -> String {
    let sequence = UNIQUE_SUFFIX_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!("{nanos}-{}-{sequence}", std::process::id())
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::sync::{Arc, Barrier};
    use std::thread;

    use super::unique_suffix_at;

    #[test]
    fn temporary_suffixes_are_unique_across_concurrent_callers() {
        const WORKERS: usize = 8;
        const ROUNDS: usize = 1_000;

        let barrier = Arc::new(Barrier::new(WORKERS));
        let workers = (0..WORKERS)
            .map(|_| {
                let barrier = Arc::clone(&barrier);
                thread::spawn(move || {
                    (0..ROUNDS)
                        .map(|_| {
                            barrier.wait();
                            unique_suffix_at(1_000_000_000)
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect::<Vec<_>>();

        let suffixes = workers
            .into_iter()
            .flat_map(|worker| worker.join().expect("suffix worker"))
            .collect::<Vec<_>>();
        assert_eq!(
            suffixes.iter().collect::<HashSet<_>>().len(),
            suffixes.len()
        );
    }
}
