//! The client's notes as timings: `zootr ap connect --timings` writes them
//! as JSON lines, and the bench reads latencies from them.

use std::time::{Duration, Instant};

use serde_json::json;

use super::client::Note;

/// `note` as a timings line, `at` into the run. Only the notes a latency
/// is read from: sent, confirmed by the server, and received (given).
pub fn line(at: Duration, note: &Note) -> Option<String> {
    let (event, names) = match note {
        Note::Sent(names) => ("sent", names.clone()),
        Note::Confirmed(names) => ("confirmed", names.clone()),
        Note::Received { item, .. } => ("received", vec![item.clone()]),
        _ => return None,
    };
    let ms = at.as_secs_f64() * 1000.0;
    Some(
        json!({ "ms": (ms * 1000.0).round() / 1000.0, "event": event, "names": names }).to_string(),
    )
}

/// The notes a run heard, each when it came.
pub type Heard = [(Instant, Note)];

/// The first location sent after `since`.
pub fn sent_after(heard: &Heard, since: Instant) -> Option<String> {
    heard.iter().find_map(|(at, n)| match n {
        Note::Sent(names) if *at >= since => names.first().cloned(),
        _ => None,
    })
}

/// When the server confirmed `name`, after `since`.
pub fn confirmed(heard: &Heard, name: &str, since: Instant) -> Option<Instant> {
    heard.iter().find_map(|(at, n)| match n {
        Note::Confirmed(names) if *at >= since && names.iter().any(|x| x == name) => Some(*at),
        _ => None,
    })
}

/// When the first item given after `since` was given.
pub fn received(heard: &Heard, since: Instant) -> Option<Instant> {
    heard.iter().find_map(|(at, n)| match n {
        Note::Received { .. } if *at >= since => Some(*at),
        _ => None,
    })
}

/// Whether the client has connected.
pub fn connected(heard: &Heard) -> bool {
    heard.iter().any(|(_, n)| *n == Note::Connected)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(n: &[&str]) -> Vec<String> {
        n.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn only_sent_confirmed_and_received_notes_make_lines() {
        let ms = Duration::from_micros(1_500_250);
        let sent = line(ms, &Note::Sent(names(&["Mido's House Back Left"]))).unwrap();
        assert_eq!(
            sent,
            r#"{"event":"sent","ms":1500.25,"names":["Mido's House Back Left"]}"#
        );
        let got = Note::Received {
            item: "Red Rupee (20)".into(),
            from: "Server".into(),
        };
        assert!(line(ms, &got).unwrap().contains(r#""event":"received""#));
        assert_eq!(line(ms, &Note::Connected), None);
    }

    #[test]
    fn a_check_s_latency_runs_from_its_write_to_the_server_s_confirmation() {
        let t0 = Instant::now();
        let at = |ms| t0 + Duration::from_millis(ms);
        let heard = vec![
            (at(5), Note::Confirmed(names(&["Old"]))),
            (at(10), Note::Sent(names(&["A"]))),
            (at(30), Note::Confirmed(names(&["B", "A"]))),
            (
                at(40),
                Note::Received {
                    item: "Blue Rupee (5)".into(),
                    from: "Server".into(),
                },
            ),
        ];
        assert_eq!(sent_after(&heard, at(1)), Some("A".into()));
        assert_eq!(confirmed(&heard, "A", at(1)), Some(at(30)));
        assert_eq!(confirmed(&heard, "Old", at(6)), None);
        assert_eq!(received(&heard, at(31)), Some(at(40)));
        assert_eq!(received(&heard, at(41)), None);
        assert!(!connected(&heard));
    }
}
