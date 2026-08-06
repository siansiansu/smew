//! Pane is one embedded terminal session: a command running on a PTY, with
//! its output fed into a virtual-terminal emulator (vt100) so it can be
//! rendered inside a TUI split.
//!
//! Concurrency: the PTY reader thread and the UI thread share the parser via
//! a Mutex. The reader thread owns the child process and reaps it (wait)
//! after PTY EOF.

use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use portable_pty::{ChildKiller, CommandBuilder, MasterPty, PtySize, native_pty_system};

const SCROLLBACK_LINES: usize = 5000;

/// Coalesced "pane output / exited" callback, invoked from the reader thread.
pub type Notifier = Arc<dyn Fn() + Send + Sync>;

pub struct Pane {
    pub title: String,

    parser: Arc<Mutex<vt100::Parser>>,
    writer: Arc<Mutex<Option<Box<dyn Write + Send>>>>,
    master: Mutex<Option<Box<dyn MasterPty + Send>>>,
    killer: Mutex<Box<dyn ChildKiller + Send + Sync>>,
    /// Process-group id of the spawned child (it is made a session leader by
    /// the PTY spawn), used by close() to kill the whole tree.
    pid: Option<u32>,
    done: Arc<AtomicBool>,
}

impl Pane {
    /// Launches argv on a PTY sized cols×rows and starts reading its output
    /// into the emulator. notify is called whenever output arrives or the
    /// process exits, so the UI can re-render.
    pub fn start(
        title: &str,
        argv: &[String],
        cols: u16,
        rows: u16,
        notify: Notifier,
    ) -> Result<Arc<Pane>, String> {
        let (cols, rows) = (cols.max(1), rows.max(1));
        let pty = native_pty_system();
        let pair = pty
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| e.to_string())?;

        let Some((prog, rest)) = argv.split_first() else {
            return Err("empty command".to_string());
        };
        let mut cmd = CommandBuilder::new(prog);
        cmd.args(rest);
        let mut child = pair.slave.spawn_command(cmd).map_err(|e| e.to_string())?;
        drop(pair.slave);

        let killer = child.clone_killer();
        let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
        let writer = pair.master.take_writer().map_err(|e| e.to_string())?;

        let pane = Arc::new(Pane {
            title: title.to_string(),
            parser: Arc::new(Mutex::new(vt100::Parser::new(rows, cols, SCROLLBACK_LINES))),
            writer: Arc::new(Mutex::new(Some(writer))),
            master: Mutex::new(Some(pair.master)),
            killer: Mutex::new(killer),
            pid: child.process_id(),
            done: Arc::new(AtomicBool::new(false)),
        });

        // Reader thread: PTY output → emulator (+ query replies), then notify.
        // After EOF it reaps the child so it doesn't linger as a zombie.
        let parser = Arc::clone(&pane.parser);
        let writer = Arc::clone(&pane.writer);
        let done = Arc::clone(&pane.done);
        std::thread::spawn(move || {
            let mut responder = Responder::default();
            let mut buf = [0u8; 8192];
            loop {
                match reader.read(&mut buf) {
                    Ok(n) if n > 0 => {
                        let replies = {
                            let mut p = parser.lock().unwrap();
                            process_and_respond(&mut p, &mut responder, &buf[..n])
                        };
                        if !replies.is_empty()
                            && let Some(w) = writer.lock().unwrap().as_mut()
                        {
                            let _ = w.write_all(&replies);
                            let _ = w.flush();
                        }
                        notify();
                    }
                    _ => {
                        // EOF (or EIO once the child is gone): the session ended.
                        done.store(true, Ordering::SeqCst);
                        notify();
                        let _ = child.wait(); // reap the child (no zombie)
                        return;
                    }
                }
            }
        });

        Ok(pane)
    }

    /// Sends input bytes to the session's PTY (keyboard → remote shell).
    pub fn write(&self, b: &[u8]) {
        if b.is_empty() {
            return;
        }
        if let Some(w) = self.writer.lock().unwrap().as_mut() {
            let _ = w.write_all(b);
            let _ = w.flush();
        }
    }

    /// Updates both the emulator and the PTY window size.
    pub fn resize(&self, cols: u16, rows: u16) {
        let (cols, rows) = (cols.max(1), rows.max(1));
        self.parser
            .lock()
            .unwrap()
            .screen_mut()
            .set_size(rows, cols);
        if let Some(m) = self.master.lock().unwrap().as_ref() {
            let _ = m.resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            });
        }
    }

    /// Runs f against the screen scrolled up by `off` lines into scrollback
    /// (0 = the live screen). The offset is reset before returning, so views
    /// are stateless.
    pub fn with_screen<R>(&self, off: usize, f: impl FnOnce(&vt100::Screen) -> R) -> R {
        let mut p = self.parser.lock().unwrap();
        p.screen_mut().set_scrollback(off);
        let r = f(p.screen());
        p.screen_mut().set_scrollback(0);
        r
    }

    /// The emulator's cursor position (column, row).
    pub fn cursor_pos(&self) -> (u16, u16) {
        let p = self.parser.lock().unwrap();
        let (row, col) = p.screen().cursor_position();
        (col, row)
    }

    /// The emulator screen height.
    pub fn rows(&self) -> u16 {
        self.parser.lock().unwrap().screen().size().0
    }

    /// Whether arrows/home/end must be encoded in application mode (DECCKM),
    /// as enabled by full-screen apps like less and vim.
    pub fn application_cursor(&self) -> bool {
        self.parser.lock().unwrap().screen().application_cursor()
    }

    /// How many lines of scrollback history are available.
    pub fn scrollback_len(&self) -> usize {
        let mut p = self.parser.lock().unwrap();
        let cur = p.screen().scrollback();
        p.screen_mut().set_scrollback(usize::MAX); // clamps to the history size
        let len = p.screen().scrollback();
        p.screen_mut().set_scrollback(cur);
        len
    }

    /// The current screen as plain text (used by tests and last_line).
    pub fn contents_text(&self) -> String {
        self.parser.lock().unwrap().screen().contents()
    }

    /// The last non-blank screen line — used as a status note when the
    /// process exits (the SSM "Exiting session…" line on a normal exit, or
    /// the aws CLI error when the session failed to start).
    pub fn last_line(&self) -> String {
        let contents = self.contents_text();
        contents
            .lines()
            .rev()
            .map(str::trim)
            .find(|l| !l.is_empty())
            .unwrap_or_default()
            .to_string()
    }

    /// Whether the underlying process has exited.
    pub fn is_done(&self) -> bool {
        self.done.load(Ordering::SeqCst)
    }

    /// Kills the process tree and tears down the PTY. The PTY spawn made the
    /// child a session leader, so killing its process group also reaches
    /// descendants that hold the PTY slave (e.g. session-manager-plugin
    /// under the aws CLI) — otherwise the reader thread would block forever
    /// and the direct child would never be reaped. Skipped once done: the
    /// process is already gone and its pid may have been reused.
    pub fn close(&self) {
        if !self.is_done() {
            #[cfg(unix)]
            if let Some(pid) = self.pid {
                unsafe { libc::kill(-(pid as i32), libc::SIGKILL) };
            }
            let _ = self.killer.lock().unwrap().kill();
        }
        *self.writer.lock().unwrap() = None;
        *self.master.lock().unwrap() = None;
    }
}

/// Recognizes the terminal queries that full-screen apps send on startup.
/// Without replies, apps like vim wait on DA / DSR answers (vt100 itself
/// never responds to queries). The matcher is a byte-driven state machine,
/// so sequences split across read chunks are still recognized.
#[derive(Default)]
struct Responder {
    state: ResponderState,
    params: Vec<u8>,
}

#[derive(Default, PartialEq)]
enum ResponderState {
    #[default]
    Idle,
    Esc,
    Csi,
}

/// A terminal query that needs an answer.
enum Query {
    Da1,  // ESC[c / ESC[0c / ESC Z → device attributes
    Da2,  // ESC[>c → secondary device attributes
    Dsr5, // ESC[5n → status report
    Cpr,  // ESC[6n → cursor position report
    XCpr, // ESC[?6n → DEC extended cursor position report
}

impl Responder {
    /// Feeds one byte; returns a query when this byte completes one.
    fn scan_byte(&mut self, b: u8) -> Option<Query> {
        match self.state {
            ResponderState::Idle => {
                if b == 0x1b {
                    self.state = ResponderState::Esc;
                }
                None
            }
            ResponderState::Esc => match b {
                b'[' => {
                    self.state = ResponderState::Csi;
                    self.params.clear();
                    None
                }
                b'Z' => {
                    // DECID — answered like DA1.
                    self.state = ResponderState::Idle;
                    Some(Query::Da1)
                }
                0x1b => None, // stay: ESC ESC — still at an escape start
                _ => {
                    self.state = ResponderState::Idle;
                    None
                }
            },
            ResponderState::Csi => match b {
                b'0'..=b'9' | b';' | b'?' | b'>' | b'=' => {
                    if self.params.len() < 8 {
                        self.params.push(b);
                    } else {
                        self.state = ResponderState::Idle; // not a query we answer
                    }
                    None
                }
                b'c' => {
                    let q = match self.params.as_slice() {
                        b"" | b"0" => Some(Query::Da1),
                        b">" | b">0" => Some(Query::Da2),
                        _ => None,
                    };
                    self.state = ResponderState::Idle;
                    q
                }
                b'n' => {
                    let q = match self.params.as_slice() {
                        b"5" => Some(Query::Dsr5),
                        b"6" => Some(Query::Cpr),
                        b"?6" => Some(Query::XCpr),
                        _ => None,
                    };
                    self.state = ResponderState::Idle;
                    q
                }
                _ => {
                    self.state = ResponderState::Idle; // any other final byte
                    None
                }
            },
        }
    }
}

/// Feeds a PTY chunk into the emulator and collects replies for any terminal
/// queries in it. Cursor-position reports are answered with the cursor as of
/// the query byte — the bytes up to and including the query are processed
/// first, trailing bytes after — matching what a real terminal reports.
fn process_and_respond(
    parser: &mut vt100::Parser,
    responder: &mut Responder,
    chunk: &[u8],
) -> Vec<u8> {
    let mut out = Vec::new();
    let mut flushed = 0;
    for (i, &b) in chunk.iter().enumerate() {
        let Some(q) = responder.scan_byte(b) else {
            continue;
        };
        match q {
            Query::Da1 => out.extend_from_slice(b"\x1b[?6c"), // VT102
            Query::Da2 => out.extend_from_slice(b"\x1b[>0;95;0c"),
            Query::Dsr5 => out.extend_from_slice(b"\x1b[0n"), // status: OK
            Query::Cpr | Query::XCpr => {
                parser.process(&chunk[flushed..=i]);
                flushed = i + 1;
                let (row, col) = parser.screen().cursor_position();
                let reply = match q {
                    Query::Cpr => format!("\x1b[{};{}R", row + 1, col + 1),
                    _ => format!("\x1b[?{};{};1R", row + 1, col + 1),
                };
                out.extend_from_slice(reply.as_bytes());
            }
        }
    }
    parser.process(&chunk[flushed..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn no_notify() -> Notifier {
        Arc::new(|| {})
    }

    fn argv(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    /// Runs a script on a pane and waits until the pane reports done.
    fn start_exited(script: &str) -> Arc<Pane> {
        let p = Pane::start("test", &argv(&["sh", "-c", script]), 20, 5, no_notify()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !p.is_done() {
            assert!(
                Instant::now() < deadline,
                "pane never reported done after the process exited"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        p
    }

    #[test]
    fn pane_done_on_exit() {
        let p = start_exited("exit 0");
        p.close();
    }

    #[test]
    fn pane_last_line() {
        let p = start_exited("echo hello-last-line");
        assert!(
            p.last_line().contains("hello-last-line"),
            "last_line() = {:?}, want it to contain hello-last-line",
            p.last_line()
        );
        p.close();
    }

    #[test]
    fn pane_write_and_render() {
        let p = Pane::start("cat", &argv(&["cat"]), 40, 5, no_notify()).unwrap();
        p.write(b"marker-123\r");
        let deadline = Instant::now() + Duration::from_secs(3);
        while !p.contents_text().contains("marker-123") {
            assert!(Instant::now() < deadline, "render: {:?}", p.contents_text());
            std::thread::sleep(Duration::from_millis(20));
        }
        p.close();
    }

    #[test]
    fn pane_resize_and_scrollback() {
        let p = Pane::start(
            "sh",
            &argv(&[
                "sh",
                "-c",
                "for i in $(seq 1 40); do echo line-$i; done; sleep 5",
            ]),
            20,
            5,
            no_notify(),
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        while !p.contents_text().contains("line-40") {
            assert!(Instant::now() < deadline, "render: {:?}", p.contents_text());
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(
            p.scrollback_len() > 0,
            "expected scrollback after 40 lines on a 5-row pane"
        );
        // A scrolled view shows older lines; offset resets afterwards.
        let scrolled = p.with_screen(p.scrollback_len(), |s| s.contents());
        assert!(scrolled.contains("line-1"), "scrolled view: {scrolled:?}");
        let live = p.contents_text();
        assert!(
            live.contains("line-40"),
            "live view after scroll reset: {live:?}"
        );
        p.resize(30, 8);
        assert_eq!(p.rows(), 8);
        p.close();
    }

    /// Test harness: a parser primed with "ab" (cursor at row 0, col 2) and
    /// a persistent responder, mimicking the reader thread's loop.
    struct Harness {
        parser: vt100::Parser,
        responder: Responder,
    }

    impl Harness {
        fn new() -> Self {
            let mut parser = vt100::Parser::new(5, 20, 0);
            parser.process(b"ab");
            Self {
                parser,
                responder: Responder::default(),
            }
        }
        fn scan(&mut self, chunk: &[u8]) -> Vec<u8> {
            process_and_respond(&mut self.parser, &mut self.responder, chunk)
        }
    }

    #[test]
    fn responder_answers_queries() {
        let mut h = Harness::new();
        assert_eq!(h.scan(b"\x1b[c"), b"\x1b[?6c");
        assert_eq!(h.scan(b"\x1b[0c"), b"\x1b[?6c");
        assert_eq!(h.scan(b"\x1b[>c"), b"\x1b[>0;95;0c");
        assert_eq!(h.scan(b"\x1b[5n"), b"\x1b[0n");
        assert_eq!(h.scan(b"\x1b[6n"), b"\x1b[1;3R");
        assert_eq!(h.scan(b"\x1b[31mred\x1b[6n"), b"\x1b[1;6R"); // after "red"
    }

    // A CPR must report the cursor as of the query byte, not after trailing
    // output in the same chunk (a real terminal answers at query time).
    #[test]
    fn responder_cpr_position_at_query_time() {
        let mut h = Harness::new();
        assert_eq!(h.scan(b"\x1b[6nxyz"), b"\x1b[1;3R"); // col 3, not col 6
        // ...and the trailing bytes still reached the emulator.
        assert!(h.parser.screen().contents().contains("abxyz"));
    }

    #[test]
    fn responder_handles_split_sequences() {
        let mut h = Harness::new();
        assert_eq!(h.scan(b"\x1b"), b"");
        assert_eq!(h.scan(b"["), b"");
        assert_eq!(h.scan(b"6n"), b"\x1b[1;3R");
    }

    #[test]
    fn responder_ignores_non_queries() {
        let mut h = Harness::new();
        assert_eq!(h.scan(b"\x1b[2J\x1b[H\x1b[1;31mhi\x1b[0m"), b"");
        assert_eq!(h.scan(b"\x1b[38;5;196mx"), b""); // params too long → dropped
    }

    // close() must kill the whole process group: a background child that
    // still holds the PTY slave (like session-manager-plugin under the aws
    // CLI) would otherwise keep the reader thread blocked forever and the
    // direct child would never be reaped.
    #[test]
    fn close_kills_descendants_holding_the_pty() {
        let p = Pane::start(
            "tree",
            &argv(&["sh", "-c", "sleep 30 & exec sleep 31"]),
            20,
            5,
            no_notify(),
        )
        .unwrap();
        assert!(!p.is_done());
        p.close();
        let deadline = Instant::now() + Duration::from_secs(3);
        while !p.is_done() {
            assert!(
                Instant::now() < deadline,
                "reader thread never unblocked after close() — descendants kept the PTY open"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}
