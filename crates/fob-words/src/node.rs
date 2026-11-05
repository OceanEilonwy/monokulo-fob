//! The Node screen's checks, what syncing says of a failing node, and what
//! instant updates say when they can't connect.

use fob_chain::{
    error::EndpointProblem,
    node::{ANSWER, Failure, Line, Outcome, Row, Step, SwitchRefused, Verdict},
    transport::CertProblem,
};
use fob_monero::{amount::grouped, network::{ZmqProblem, link_speed}};

use crate::capitalised;

/// What Fob says when asked for a node on an onion service.
pub const NO_ONION: &str = "Fob can't reach .onion nodes yet";

/// "Right network".
pub fn title(step: Step) -> &'static str {
    match step {
        Step::Certificate => "Certificate",
        Step::Answers => "Answers",
        Step::Network => "Right network",
        Step::UpToDate => "Up to date",
        Step::Blocks => "Sends blocks to Fob",
    }
}

/// "Working", "Checking…", or the first failed check's words.
pub fn verdict(verdict: Verdict) -> &'static str {
    match verdict {
        Verdict::Checking => "Checking…",
        Verdict::Working => "Working",
        Verdict::Failed(Step::Certificate) => "Certificate not accepted",
        Verdict::Failed(Step::Answers) => "Doesn't answer",
        Verdict::Failed(Step::Network) => "Wrong network",
        Verdict::Failed(Step::UpToDate) => "Not up to date",
        Verdict::Failed(Step::Blocks) => "Doesn't send blocks",
    }
}

/// "self-signed", for "self-signed, not accepted".
pub fn certificate(problem: CertProblem) -> &'static str {
    match problem {
        CertProblem::SelfSigned => "self-signed",
        CertProblem::Expired => "expired",
        CertProblem::NotYetValid => "not valid yet",
        CertProblem::OtherName => "for another address",
        CertProblem::BadSignature => "signature doesn't match",
        CertProblem::Invalid => "not valid",
    }
}

/// "didn't answer in 5 s", "self-signed, not accepted".
pub fn failure(failure: &Failure, limit: u64) -> String {
    match failure {
        Failure::Onion => "needs Tor, not in Fob yet".into(),
        Failure::BadAddress => "not an address Fob can read".into(),
        Failure::Certificate(problem) => format!("{}, not accepted", certificate(*problem)),
        Failure::TimedOut => format!("didn't answer in {limit} s"),
        Failure::Refused => "refused to connect".into(),
        Failure::NotFound => "address not found".into(),
        Failure::NotMonero => "not a Monero node".into(),
        // monerod started with --rpc-login.
        Failure::Status(401) => "wants a login".into(),
        Failure::Status(code) => format!("answered {code}"),
        Failure::Unreadable => "sent blocks Fob can't read".into(),
        Failure::Other => "couldn't connect".into(),
    }
}

/// "just now", "1 min ago", "2 h ago".
fn ago(seconds: u64) -> String {
    match seconds {
        0..60 => "just now".into(),
        60..3600 => format!("{} min ago", seconds / 60),
        _ => format!("{} h ago", seconds / 3600),
    }
}

/// "20 min behind", "5 h behind", "2 days behind".
fn behind(seconds: u64) -> String {
    match seconds {
        0..3600 => format!("{} min behind", (seconds / 60).max(1)),
        3600..172_800 => format!("{} h behind", seconds / 3600),
        _ => format!("{} days behind", seconds / 86_400),
    }
}

/// A check's result: "84 ms", "stagenet", "block 1,792,004 · 1 min ago",
/// "2 days behind", "31 blocks/s". Nothing for one not run.
pub fn result(outcome: &Outcome) -> String {
    match outcome {
        Outcome::Checking => "checking…".into(),
        Outcome::Skipped => String::new(),
        Outcome::Certificate(None) => "valid".into(),
        Outcome::Certificate(Some(problem)) => format!("{}, accepted", certificate(*problem)),
        Outcome::Latency(ms) => format!("{ms} ms"),
        Outcome::Network(network) => network.clone(),
        Outcome::WrongNetwork { theirs, ours } => {
            let theirs = if theirs.is_empty() { "unknown network" } else { theirs };
            format!("{theirs}, not {ours}")
        }
        Outcome::Block { height, age } => format!("block {} · {}", grouped(*height), ago(*age)),
        Outcome::Behind { seconds } => behind(*seconds),
        Outcome::Failed { failure: why, limit } => failure(why, *limit),
        Outcome::NoBlocks => "sent no blocks".into(),
        Outcome::NoBlocksWithin { seconds } => format!("no blocks in {seconds} s"),
        Outcome::Speed { blocks_per_second, .. } => {
            let blocks = if *blocks_per_second >= 9.95 { grouped(blocks_per_second.round() as u64) } else { format!("{blocks_per_second:.1}") };
            format!("{blocks} blocks/s")
        }
    }
}

/// A line of its own under the result, never beside it: the link speed,
/// "1.8 MB/s", under "31 blocks/s", so a phone never wraps one inside its value.
pub fn second(outcome: &Outcome) -> Option<String> {
    match outcome {
        Outcome::Speed { bytes_per_second, .. } => Some(link_speed(*bytes_per_second)),
        _ => None,
    }
}

/// For TalkBack: "Up to date: 2 days behind", "Answers: not checked",
/// "Sends blocks to Fob: 31 blocks/s, 1.8 MB/s".
pub fn label(row: &Row) -> String {
    let title = title(row.step);
    let said = result(&row.outcome);
    match (&row.outcome, second(&row.outcome)) {
        (Outcome::Skipped, _) => format!("{title}: not checked"),
        (_, Some(second)) => format!("{title}: {said}, {second}"),
        (_, None) => format!("{title}: {said}"),
    }
}

/// One line for the list of other nodes: "120 ms · in sync · fastest",
/// "Didn't answer in 5 s", "Mainnet node: this wallet is stagenet".
pub fn line(line: &Line) -> String {
    match line {
        Line::Checking => "Checking…".into(),
        Line::InSync { ms, fastest } => format!("{ms} ms · in sync{}", if *fastest { " · fastest" } else { "" }),
        Line::Certificate(outcome) => format!("Certificate {}", result(outcome)),
        Line::NoAnswer(outcome) => capitalised(&result(outcome)),
        Line::OtherNetwork { theirs, ours } if theirs.is_empty() => format!("Unknown network: this wallet is {ours}"),
        Line::OtherNetwork { theirs, ours } => format!("{} node: this wallet is {ours}", capitalised(theirs)),
        Line::Poor { ms, outcome } => format!("{ms} ms · {}", result(outcome)),
    }
}

/// The sync card's words for a failed sync step, as the node checks would
/// say it ("Address not found", "Certificate expired, not accepted").
pub fn sync_failure(why: &Failure) -> String {
    let limit = ANSWER.as_secs();
    match why {
        // Syncing's limits are its own (15 s to connect, 30 s quiet).
        Failure::TimedOut => "Stopped answering".into(),
        Failure::Certificate(_) => format!("Certificate {}", failure(why, limit)),
        why => capitalised(&failure(why, limit)),
    }
}

/// Why switching to a node was refused.
pub fn switch_refused(refused: &SwitchRefused) -> String {
    match refused {
        SwitchRefused::Endpoint(EndpointProblem::Onion) => NO_ONION.to_owned(),
        SwitchRefused::Endpoint(other) => format!("{other:?}"),
        SwitchRefused::Failed(why) => failure(why, ANSWER.as_secs()),
        SwitchRefused::OtherNetwork { theirs, ours } => {
            let theirs = if theirs.is_empty() { "an unknown network" } else { theirs };
            format!("that node is on {theirs}, and this wallet is on {ours}")
        }
    }
}

/// A failure in a few words, for Settings.
pub fn zmq_problem(problem: &ZmqProblem) -> String {
    match problem {
        ZmqProblem::NothingListening => "nothing listening on that port".into(),
        ZmqProblem::NoAnswer => "no answer".into(),
        ZmqProblem::NodeClosed => "the node closed the connection".into(),
        ZmqProblem::HostNotFound => "host not found".into(),
        ZmqProblem::Onion => NO_ONION.into(),
        ZmqProblem::Other(text) => text.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn failed(failure: Failure, limit: u64) -> Outcome {
        Outcome::Failed { failure, limit }
    }

    fn row(step: Step, outcome: Outcome) -> Row {
        Row { step, outcome, mark: fob_chain::node::Mark::Pass }
    }

    const SPEED: Outcome = Outcome::Speed { blocks_per_second: 31.0, bytes_per_second: 1_800_000.0 };

    #[test]
    fn each_check_has_its_title_and_verdict() {
        assert_eq!([Step::Certificate, Step::Answers, Step::Network, Step::UpToDate, Step::Blocks].map(title), ["Certificate", "Answers", "Right network", "Up to date", "Sends blocks to Fob"]);
        assert_eq!(verdict(Verdict::Working), "Working");
        assert_eq!(verdict(Verdict::Checking), "Checking…");
        assert_eq!(verdict(Verdict::Failed(Step::Certificate)), "Certificate not accepted");
        assert_eq!(verdict(Verdict::Failed(Step::Answers)), "Doesn't answer");
        assert_eq!(verdict(Verdict::Failed(Step::Network)), "Wrong network");
        assert_eq!(verdict(Verdict::Failed(Step::UpToDate)), "Not up to date");
        assert_eq!(verdict(Verdict::Failed(Step::Blocks)), "Doesn't send blocks");
    }

    #[test]
    fn results_say_what_each_check_found() {
        assert_eq!(result(&Outcome::Checking), "checking…");
        assert_eq!(result(&Outcome::Skipped), "");
        assert_eq!(result(&Outcome::Latency(84)), "84 ms");
        assert_eq!(result(&Outcome::Network("stagenet".into())), "stagenet");
        assert_eq!(result(&Outcome::WrongNetwork { theirs: "mainnet".into(), ours: "stagenet".into() }), "mainnet, not stagenet");
        assert_eq!(result(&Outcome::WrongNetwork { theirs: String::new(), ours: "stagenet".into() }), "unknown network, not stagenet");
        assert_eq!(result(&Outcome::Block { height: 1_792_004, age: 75 }), "block 1,792,004 · 1 min ago");
        assert_eq!(result(&Outcome::Block { height: 1_792_004, age: 3 }), "block 1,792,004 · just now");
        assert_eq!(result(&Outcome::Behind { seconds: 172_800 }), "2 days behind");
        assert_eq!(result(&Outcome::Behind { seconds: 3600 }), "1 h behind");
        assert_eq!(result(&Outcome::Behind { seconds: 3 * 3600 }), "3 h behind");
        assert_eq!(result(&Outcome::Behind { seconds: 108_000 }), "30 h behind");
        assert_eq!(result(&Outcome::NoBlocks), "sent no blocks");
        assert_eq!(result(&Outcome::NoBlocksWithin { seconds: 10 }), "no blocks in 10 s");
        assert_eq!(result(&failed(Failure::TimedOut, 5)), "didn't answer in 5 s");
        assert_eq!(result(&failed(Failure::Unreadable, 10)), "sent blocks Fob can't read");
        assert_eq!(result(&failed(Failure::Refused, 5)), "refused to connect");
        assert_eq!(result(&failed(Failure::Onion, 5)), "needs Tor, not in Fob yet");
        assert_eq!(result(&failed(Failure::Status(307), 5)), "answered 307");
    }

    #[test]
    fn certificates_say_whether_they_were_accepted() {
        assert_eq!(result(&Outcome::Certificate(None)), "valid");
        assert_eq!(result(&Outcome::Certificate(Some(CertProblem::SelfSigned))), "self-signed, accepted");
        assert_eq!(result(&failed(Failure::Certificate(CertProblem::SelfSigned), 5)), "self-signed, not accepted");
        assert_eq!(result(&failed(Failure::Certificate(CertProblem::BadSignature), 5)), "signature doesn't match, not accepted");
        assert_eq!(failure(&Failure::Certificate(CertProblem::Invalid), 5), "not valid, not accepted");
        assert_eq!([CertProblem::Expired, CertProblem::NotYetValid, CertProblem::OtherName].map(certificate), ["expired", "not valid yet", "for another address"]);
    }

    #[test]
    fn speeds_take_two_lines() {
        assert_eq!((result(&SPEED), second(&SPEED).as_deref()), ("31 blocks/s".into(), Some("1.8 MB/s")));
        let slow = Outcome::Speed { blocks_per_second: 5.0, bytes_per_second: 40_000.0 };
        assert_eq!((result(&slow), second(&slow).as_deref()), ("5.0 blocks/s".into(), Some("40 kB/s")));
        assert_eq!(second(&Outcome::Latency(1)), None);
    }

    #[test]
    fn labels_read_a_row_aloud() {
        assert_eq!(label(&row(Step::Blocks, SPEED)), "Sends blocks to Fob: 31 blocks/s, 1.8 MB/s");
        assert_eq!(label(&row(Step::UpToDate, Outcome::Block { height: 1_792_004, age: 75 })), "Up to date: block 1,792,004 · 1 min ago");
        assert_eq!(label(&row(Step::Network, Outcome::Skipped)), "Right network: not checked");
        assert_eq!(label(&row(Step::Blocks, Outcome::NoBlocksWithin { seconds: 10 })), "Sends blocks to Fob: no blocks in 10 s");
    }

    #[test]
    fn a_node_on_the_list_has_one_line() {
        assert_eq!(line(&Line::Checking), "Checking…");
        assert_eq!(line(&Line::InSync { ms: 0, fastest: false }), "0 ms · in sync");
        assert_eq!(line(&Line::InSync { ms: 0, fastest: true }), "0 ms · in sync · fastest");
        assert_eq!(line(&Line::NoAnswer(failed(Failure::TimedOut, 5))), "Didn't answer in 5 s");
        assert_eq!(line(&Line::NoAnswer(failed(Failure::Refused, 5))), "Refused to connect");
        assert_eq!(line(&Line::NoAnswer(failed(Failure::NotMonero, 5))), "Not a Monero node");
        assert_eq!(line(&Line::NoAnswer(failed(Failure::Onion, 5))), "Needs Tor, not in Fob yet");
        assert_eq!(line(&Line::OtherNetwork { theirs: "mainnet".into(), ours: "stagenet".into() }), "Mainnet node: this wallet is stagenet");
        assert_eq!(line(&Line::OtherNetwork { theirs: String::new(), ours: "stagenet".into() }), "Unknown network: this wallet is stagenet");
        assert_eq!(line(&Line::Poor { ms: 0, outcome: Outcome::Behind { seconds: 172_800 } }), "0 ms · 2 days behind");
        assert_eq!(line(&Line::Poor { ms: 0, outcome: Outcome::Behind { seconds: 108_000 } }), "0 ms · 30 h behind");
        assert_eq!(line(&Line::Poor { ms: 0, outcome: Outcome::NoBlocksWithin { seconds: 10 } }), "0 ms · no blocks in 10 s");
        assert_eq!(line(&Line::Certificate(failed(Failure::Certificate(CertProblem::SelfSigned), 5))), "Certificate self-signed, not accepted");
        assert_eq!(line(&Line::Certificate(failed(Failure::Certificate(CertProblem::BadSignature), 5))), "Certificate signature doesn't match, not accepted");
    }

    #[test]
    fn times_read_as_people_say_them() {
        assert_eq!([ago(10), ago(75), ago(3599), ago(3600)], ["just now", "1 min ago", "59 min ago", "1 h ago"]);
        assert_eq!([behind(30), behind(720), behind(7200), behind(172_800)], ["1 min behind", "12 min behind", "2 h behind", "2 days behind"]);
        assert_eq!(failure(&Failure::Status(401), 5), "wants a login");
        assert_eq!(failure(&Failure::BadAddress, 5), "not an address Fob can read");
        assert_eq!(failure(&Failure::NotFound, 5), "address not found");
        assert_eq!(failure(&Failure::Other, 5), "couldn't connect");
        assert_eq!(capitalised("éa"), "Éa");
    }

    /// The sync card says why syncing failed as the checks would, never the
    /// transport's debug text.
    #[test]
    fn sync_failures_are_told_in_words() {
        assert_eq!(sync_failure(&Failure::NotFound), "Address not found");
        assert_eq!(sync_failure(&Failure::Refused), "Refused to connect");
        assert_eq!(sync_failure(&Failure::TimedOut), "Stopped answering");
        assert_eq!(sync_failure(&Failure::Status(401)), "Wants a login");
        assert_eq!(sync_failure(&Failure::Unreadable), "Sent blocks Fob can't read");
        assert_eq!(sync_failure(&Failure::Other), "Couldn't connect");
        assert_eq!(sync_failure(&Failure::Onion), "Needs Tor, not in Fob yet");
        assert_eq!(sync_failure(&Failure::Certificate(CertProblem::Expired)), "Certificate expired, not accepted");
        assert_eq!(sync_failure(&Failure::Certificate(CertProblem::SelfSigned)), "Certificate self-signed, not accepted");
        assert_eq!(sync_failure(&Failure::Certificate(CertProblem::BadSignature)), "Certificate signature doesn't match, not accepted");
    }

    #[test]
    fn a_refused_switch_says_why() {
        assert_eq!(switch_refused(&SwitchRefused::Endpoint(EndpointProblem::Onion)), NO_ONION);
        assert_eq!(switch_refused(&SwitchRefused::Failed(Failure::Certificate(CertProblem::SelfSigned))), "self-signed, not accepted");
        assert_eq!(switch_refused(&SwitchRefused::OtherNetwork { theirs: "mainnet".into(), ours: "stagenet".into() }), "that node is on mainnet, and this wallet is on stagenet");
        assert_eq!(switch_refused(&SwitchRefused::OtherNetwork { theirs: String::new(), ours: "stagenet".into() }), "that node is on an unknown network, and this wallet is on stagenet");
    }

    #[test]
    fn instant_updates_say_what_stopped_them() {
        assert_eq!(zmq_problem(&ZmqProblem::NothingListening), "nothing listening on that port");
        assert_eq!(zmq_problem(&ZmqProblem::NoAnswer), "no answer");
        assert_eq!(zmq_problem(&ZmqProblem::NodeClosed), "the node closed the connection");
        assert_eq!(zmq_problem(&ZmqProblem::HostNotFound), "host not found");
        assert_eq!(zmq_problem(&ZmqProblem::Onion), "Fob can't reach .onion nodes yet");
        assert_eq!(zmq_problem(&ZmqProblem::Other("os said so".into())), "os said so");
    }
}
