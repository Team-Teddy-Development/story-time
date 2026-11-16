//! A second try for a network step that failed before it reached the other side (ST-147).
//!
//! A name that does not resolve, or a connection that is refused, fails before any request is
//! sent. Nothing happened on the forge, so the step can run again with no second effect. A
//! step that got an answer is never run again here.

use std::time::Duration;

/// The number of times a step runs in total when each failure is transient.
pub(super) const TRIES: u32 = 4;
const PAUSE: Duration = Duration::from_millis(300);

/// Runs `step` until it succeeds, fails with a failure that is not `transient`, or has run
/// `TRIES` times. The pause before try `n + 1` is `n` times `pause`.
pub(super) fn again<T, E>(
    pause: Duration,
    transient: impl Fn(&E) -> bool,
    mut step: impl FnMut() -> Result<T, E>,
) -> Result<T, E> {
    let mut tried = 1;
    loop {
        match step() {
            Err(failure) if tried < TRIES && transient(&failure) => {
                std::thread::sleep(pause * tried);
                tried += 1;
            }
            result => return result,
        }
    }
}

/// Sends `request`, and sends it again when no connection was made. A request whose body
/// cannot be copied is sent one time.
pub(super) fn send(
    request: reqwest::blocking::RequestBuilder,
) -> reqwest::Result<reqwest::blocking::Response> {
    let mut request = request;
    for tried in 1..TRIES {
        let Some(copy) = request.try_clone() else {
            break;
        };
        match request.send() {
            Err(failure) if failure.is_connect() => std::thread::sleep(PAUSE * tried),
            result => return result,
        }
        request = copy;
    }
    request.send()
}

/// Whether the text that `git` wrote says that a host name did not resolve.
pub(super) fn name_did_not_resolve(stderr: &str) -> bool {
    stderr.contains("Could not resolve host")
}

/// The pause between two tries of a `git` network command.
pub(super) const GIT_PAUSE: Duration = PAUSE;

#[cfg(test)]
mod tests {
    use super::*;

    fn run(results: &[Result<u8, &'static str>]) -> (Result<u8, &'static str>, usize) {
        let mut next = 0;
        let result = again(
            Duration::ZERO,
            |failure: &&str| *failure == "dns",
            || {
                next += 1;
                results[next - 1]
            },
        );
        (result, next)
    }

    #[test]
    fn a_transient_failure_is_tried_four_times_and_no_more() {
        assert_eq!(run(&[Err("dns"); 6]), (Err("dns"), 4));
    }

    #[test]
    fn a_success_after_transient_failures_returns_its_value() {
        assert_eq!(run(&[Err("dns"), Err("dns"), Ok(7)]), (Ok(7), 3));
        assert_eq!(run(&[Ok(1)]), (Ok(1), 1));
    }

    #[test]
    fn a_failure_with_another_cause_is_not_tried_again() {
        assert_eq!(run(&[Err("status 500"), Ok(1)]), (Err("status 500"), 1));
        assert_eq!(run(&[Err("dns"), Err("refused by policy"), Ok(1)]).1, 2);
    }

    #[test]
    fn only_the_resolver_text_of_git_counts_as_a_name_failure() {
        assert!(name_did_not_resolve(
            "fatal: unable to access 'https://github.com/o/r.git/': Could not resolve host: github.com"
        ));
        assert!(!name_did_not_resolve(
            "fatal: Authentication failed for 'https://github.com/o/r.git/'"
        ));
        assert!(!name_did_not_resolve("remote: Repository not found."));
    }

    #[test]
    fn a_request_that_cannot_connect_is_an_error_after_its_tries() {
        // Port 1 on the loopback address refuses a connection, as a missing name gives none.
        let client = reqwest::blocking::Client::new();
        let error = send(client.get("http://127.0.0.1:1/")).unwrap_err();
        assert!(error.is_connect(), "{error}");
    }
}
