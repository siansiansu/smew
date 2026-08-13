//! Session-view update logic: the leader-prefixed commands, scroll (copy)
//! mode, and the pane lifecycle (one full-screen pane per session).

mod input;
mod lifecycle;

#[cfg(test)]
mod test_util {
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use crate::session::Pane;

    pub(crate) fn no_notify() -> crate::session::Notifier {
        Arc::new(|| {})
    }

    pub(crate) fn argv(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    /// A pane whose process has already ended.
    pub(crate) fn exited_pane() -> Arc<Pane> {
        let p = Pane::start("dead", &argv(&["sh", "-c", "exit 0"]), 20, 5, no_notify()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !p.is_done() {
            assert!(Instant::now() < deadline, "pane never reported done");
            std::thread::sleep(Duration::from_millis(10));
        }
        p
    }

    /// A pane whose process keeps running for the test's duration.
    pub(crate) fn live_pane() -> Arc<Pane> {
        Pane::start("live", &argv(&["sleep", "60"]), 20, 5, no_notify()).unwrap()
    }
}
