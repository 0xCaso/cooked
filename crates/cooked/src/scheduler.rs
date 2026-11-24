//! The live feed's reader, as `Views/IslandRoot.swift` runs it: read, hand the reading to the UI, wait 5 s, again.
//! It runs on its own thread, never the UI thread. Changing the accounts reads again at once with a new source; when
//! the feed is not live it stops reading until it is again.

use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::JoinHandle;
use std::time::Duration;

use cooked_model::accounts::ConnectedAccount;
use cooked_model::clock;
use cooked_model::limit_log::LimitLog;
use cooked_model::minute_ledger::MinuteLedger;
use cooked_model::usage_source::{Reading, UsageSource};

use crate::mac::LocalZone;
use crate::paths;

/// The wait after each reading.
pub const INTERVAL: Duration = Duration::from_secs(5);

/// What the UI asks of the reader thread.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// The feed is live (read) or a demo (don't).
    Live(bool),
    /// The connected accounts changed: a new source, read at once.
    Accounts(Vec<ConnectedAccount>),
    Stop,
}

/// Makes the readings.
pub trait Reader {
    fn read(&mut self, now: f64) -> Reading;
    fn set_accounts(&mut self, accounts: Vec<ConnectedAccount>);
}

/// The time and the wait between readings; a command ends the wait early.
pub trait Ticker {
    fn now(&self) -> f64;
    /// Waits up to `timeout`; `None` when it ran out.
    fn wait(&mut self, timeout: Duration) -> Option<Command>;
}

/// The loop: one reading, then `INTERVAL`, until `Stop`.
pub fn run(
    reader: &mut dyn Reader,
    ticker: &mut dyn Ticker,
    mut live: bool,
    push: &mut dyn FnMut(Reading),
) {
    loop {
        if live {
            push(reader.read(ticker.now()));
        }
        // Waiting while not live costs one wake-up every 5 s and keeps one code path.
        match ticker.wait(INTERVAL) {
            None => {}
            Some(Command::Live(on)) => live = on,
            Some(Command::Accounts(accounts)) => reader.set_accounts(accounts),
            Some(Command::Stop) => return,
        }
    }
}

/// The real source: the accounts' logs, and the minute cache and limits store in Cooked's folder (`paths`), in the
/// user's time zone.
pub struct LiveReader {
    source: UsageSource,
}

impl LiveReader {
    pub fn new(accounts: Vec<ConnectedAccount>) -> Self {
        Self {
            source: Self::source(accounts),
        }
    }

    /// As Swift's `UsageSource` with `MinuteLedger(url:)`: each new source loads the caches from disk. The bridge
    /// folder stays the real one: Claude Code's status line writes it, Cooked only reads it.
    fn source(accounts: Vec<ConnectedAccount>) -> UsageSource {
        let dir = paths::data_dir();
        let file = |name: &str| dir.as_ref().map(|d| d.join(name));
        let ledger =
            Arc::new(file("minutes.json").map_or_else(MinuteLedger::memory, MinuteLedger::at));
        let mut source = UsageSource::new(accounts, ledger);
        source.limits = LimitLog::new(LimitLog::default_dir(), file("limits.json"));
        source
    }
}

impl Reader for LiveReader {
    fn read(&mut self, now: f64) -> Reading {
        self.source.read(now, &LocalZone::new())
    }

    fn set_accounts(&mut self, accounts: Vec<ConnectedAccount>) {
        self.source = Self::source(accounts);
    }
}

/// Wall-clock time; waits on the command channel.
struct ChannelTicker(Receiver<Command>);

impl Ticker for ChannelTicker {
    fn now(&self) -> f64 {
        clock::now()
    }

    fn wait(&mut self, timeout: Duration) -> Option<Command> {
        match self.0.recv_timeout(timeout) {
            Ok(command) => Some(command),
            Err(RecvTimeoutError::Timeout) => None,
            Err(RecvTimeoutError::Disconnected) => Some(Command::Stop),
        }
    }
}

/// The UI's side of the reader thread. Dropping it stops the thread after its current reading.
pub struct Control {
    tx: Sender<Command>,
    thread: Option<JoinHandle<()>>,
}

impl Control {
    pub fn set_live(&self, live: bool) {
        let _ = self.tx.send(Command::Live(live));
    }

    pub fn set_accounts(&self, accounts: Vec<ConnectedAccount>) {
        let _ = self.tx.send(Command::Accounts(accounts));
    }

    /// Stops the thread and waits for it: quitting lets the reading in progress finish its cache writes.
    pub fn stop(mut self) {
        let _ = self.tx.send(Command::Stop);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Starts the reader thread. `push` gets each reading on that thread; it forwards it to the UI (`OverlayHandle::update`
/// wakes the Rune loop).
pub fn spawn<R>(
    mut reader: R,
    live: bool,
    mut push: impl FnMut(Reading) + Send + 'static,
) -> Control
where
    R: Reader + Send + 'static,
{
    let (tx, rx) = mpsc::channel();
    let thread = std::thread::Builder::new()
        .name("cooked-reader".into())
        .spawn(move || run(&mut reader, &mut ChannelTicker(rx), live, &mut push))
        .ok();
    Control { tx, thread }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cooked_model::demo_data;
    use std::collections::VecDeque;

    fn reading(minute: i64) -> Reading {
        Reading {
            snapshot: demo_data::calm(),
            minute,
            history: None,
            notices: Vec::new(),
        }
    }

    /// Records when it read and which accounts it was given.
    #[derive(Default)]
    struct FakeReader {
        reads: Vec<f64>,
        accounts: Vec<Vec<ConnectedAccount>>,
    }

    impl Reader for FakeReader {
        fn read(&mut self, now: f64) -> Reading {
            self.reads.push(now);
            reading((now / 60.0) as i64)
        }

        fn set_accounts(&mut self, accounts: Vec<ConnectedAccount>) {
            self.accounts.push(accounts);
        }
    }

    /// Fake time: each wait either runs out (time moves by the whole timeout) or is cut short by the next scripted
    /// command after `after` seconds. `Stop` once the script is empty.
    struct FakeTicker {
        t: f64,
        script: VecDeque<(f64, Option<Command>)>,
        waits: Vec<Duration>,
    }

    impl FakeTicker {
        fn new(t: f64, script: Vec<(f64, Option<Command>)>) -> Self {
            Self {
                t,
                script: script.into(),
                waits: Vec::new(),
            }
        }
    }

    impl Ticker for FakeTicker {
        fn now(&self) -> f64 {
            self.t
        }

        fn wait(&mut self, timeout: Duration) -> Option<Command> {
            self.waits.push(timeout);
            match self.script.pop_front() {
                Some((_, None)) => {
                    self.t += timeout.as_secs_f64();
                    None
                }
                Some((after, command)) => {
                    self.t += after;
                    command
                }
                None => Some(Command::Stop),
            }
        }
    }

    #[test]
    fn reads_every_5s() {
        let mut reader = FakeReader::default();
        let mut ticker = FakeTicker::new(1000.0, vec![(0.0, None), (0.0, None), (0.0, None)]);
        let mut pushed = Vec::new();
        run(&mut reader, &mut ticker, true, &mut |r| {
            pushed.push(r.minute)
        });
        assert_eq!(reader.reads, vec![1000.0, 1005.0, 1010.0, 1015.0]);
        assert_eq!(pushed.len(), 4);
        assert!(ticker.waits.iter().all(|w| *w == INTERVAL));
    }

    #[test]
    fn no_readings_while_not_live() {
        let mut reader = FakeReader::default();
        let mut ticker = FakeTicker::new(
            0.0,
            vec![(0.0, None), (2.0, Some(Command::Live(true))), (0.0, None)],
        );
        let mut pushed = 0;
        run(&mut reader, &mut ticker, false, &mut |_| pushed += 1);
        // nothing at 0 and 5; live at 7 reads at once, then 5 s later
        assert_eq!(reader.reads, vec![7.0, 12.0]);
        assert_eq!(pushed, 2);
    }

    #[test]
    fn going_demo_stops_reading() {
        let mut reader = FakeReader::default();
        let mut ticker = FakeTicker::new(0.0, vec![(1.0, Some(Command::Live(false))), (0.0, None)]);
        run(&mut reader, &mut ticker, true, &mut |_| {});
        assert_eq!(reader.reads, vec![0.0]);
    }

    #[test]
    fn new_accounts_read_at_once() {
        let mut reader = FakeReader::default();
        let accounts = vec![ConnectedAccount::new("claude", "work", "/tmp/.claude")];
        let mut ticker = FakeTicker::new(
            0.0,
            vec![
                (3.0, Some(Command::Accounts(accounts.clone()))),
                (0.0, None),
            ],
        );
        run(&mut reader, &mut ticker, true, &mut |_| {});
        assert_eq!(reader.accounts, vec![accounts]);
        assert_eq!(reader.reads, vec![0.0, 3.0, 8.0]);
    }

    #[test]
    fn spawned_thread_reads_at_once_and_stops() {
        let (tx, rx) = mpsc::channel();
        let control = spawn(FakeReader::default(), true, move |r| {
            let _ = tx.send(r.minute);
        });
        assert!(rx.recv_timeout(Duration::from_secs(2)).is_ok());
        control.stop(); // ends the 5 s wait at once
        assert!(rx.recv_timeout(Duration::from_millis(100)).is_err());
    }
}
