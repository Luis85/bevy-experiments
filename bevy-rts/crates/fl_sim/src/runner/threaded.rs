//! The threaded runner: the `fl-sim` thread behind a [`RunnerPort`] (spec §5.3, ADR-0002).

use std::any::Any;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::thread::JoinHandle;

use fl_view::{FromRunner, RunnerEnd, RunnerFailure, RunnerPort, ToRunner, runner_channel};

use super::SimRunner;

impl SimRunner {
    /// Move the runner onto a new thread named `fl-sim` and return the client's end of its
    /// channels.
    ///
    /// The thread answers, in order of arrival:
    /// - [`ToRunner::Tick`] with [`FromRunner::Tick`];
    /// - [`ToRunner::Query`] with [`FromRunner::Reply`] carrying the query's id;
    /// - [`ToRunner::Control`] ([`SimRunner::control`]) with [`FromRunner::Reply`] carrying the
    ///   control's id.
    ///
    /// A message of a kind it does not know is logged and skipped.
    ///
    /// A panic while stepping or answering is caught and reported as
    /// [`FromRunner::Failed`] with the panic message and the tick that failed; the thread then
    /// exits (the match is over, spec §5.3). [`ToRunner::Shutdown`], dropping the port, or
    /// dropping its receiver also ends the thread.
    ///
    /// # Panics
    /// If the OS cannot create the thread.
    pub fn spawn_threaded(self) -> (RunnerPort, JoinHandle<()>) {
        let (port, end) = runner_channel();
        #[expect(
            clippy::disallowed_methods,
            reason = "SimRunner owns the sim thread (spec §5.3)"
        )]
        let handle = std::thread::Builder::new()
            .name("fl-sim".to_owned())
            .spawn(move || serve(self, &end))
            .expect("the sim thread starts");
        (port, handle)
    }
}

/// The thread's loop: until shutdown, a disconnected channel or a failure.
fn serve(mut runner: SimRunner, end: &RunnerEnd) {
    while let Ok(message) = end.rx.recv() {
        let tick = runner.next_tick();
        let answer = match message {
            ToRunner::Tick(input) => {
                let tick = input.tick;
                catch_unwind(AssertUnwindSafe(|| runner.step(input)))
                    .map(FromRunner::Tick)
                    .map_err(|payload| (tick, payload))
            }
            ToRunner::Query { id, query } => catch_unwind(AssertUnwindSafe(|| runner.query(query)))
                .map(|reply| FromRunner::Reply { id, reply })
                .map_err(|payload| (tick, payload)),
            ToRunner::Control { id, control } => {
                catch_unwind(AssertUnwindSafe(|| runner.control(control)))
                    .map(|reply| FromRunner::Reply { id, reply })
                    .map_err(|payload| (tick, payload))
            }
            ToRunner::Shutdown => return,
            other => {
                tracing::warn!(?other, "unsupported runner message skipped");
                continue;
            }
        };
        match answer {
            Ok(answer) => {
                if end.tx.send(answer).is_err() {
                    return;
                }
            }
            Err((tick, payload)) => {
                let message = panic_message(payload.as_ref());
                tracing::error!(tick = tick.0, %message, "the sim failed; the runner stops");
                // The client may be gone already; the thread ends either way.
                let _ = end
                    .tx
                    .send(FromRunner::Failed(RunnerFailure { tick, message }));
                return;
            }
        }
    }
}

/// The text of a panic payload (`panic!` with a literal or a format string).
fn panic_message(payload: &(dyn Any + Send)) -> String {
    if let Some(text) = payload.downcast_ref::<&str>() {
        (*text).to_owned()
    } else if let Some(text) = payload.downcast_ref::<String>() {
        text.clone()
    } else {
        "the sim panicked with a non-string payload".to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panic_messages_are_read_from_both_payload_kinds() {
        let literal = catch_unwind(|| panic!("boom")).expect_err("panics");
        assert_eq!(panic_message(literal.as_ref()), "boom");
        let formatted = catch_unwind(|| panic!("boom at {}", 5)).expect_err("panics");
        assert_eq!(panic_message(formatted.as_ref()), "boom at 5");
        let other = catch_unwind(|| std::panic::panic_any(5_u32)).expect_err("panics");
        assert_eq!(
            panic_message(other.as_ref()),
            "the sim panicked with a non-string payload"
        );
    }
}
