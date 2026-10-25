use cadence_remote::{PREFERRED_PORT, RemoteError, RemoteFailure};

pub fn failure_text(failure: RemoteFailure) -> String {
    match failure {
        RemoteFailure::DifferentServer => tr!("it is connected to a different server"),
        RemoteFailure::NotConnected => tr!("it is not connected to a server"),
        RemoteFailure::NothingToTransfer => tr!("nothing is playing"),
        RemoteFailure::VersionMismatch => tr!("it runs a different version of Cadence"),
        RemoteFailure::Busy => tr!("it is busy with another request"),
        RemoteFailure::Stopped => tr!("it stopped"),
    }
}

pub fn error_text(error: &RemoteError) -> String {
    match error {
        RemoteError::Denied => tr!("the connection was not allowed"),
        RemoteError::TimedOut => tr!("it did not answer in time"),
        RemoteError::Unreachable => tr!(
            "it could not be reached; check that its firewall allows TCP port {}",
            PREFERRED_PORT
        ),
        RemoteError::Failed(failure) => failure_text(*failure),
        RemoteError::Io(_)
        | RemoteError::Protocol(_)
        | RemoteError::Malformed(_)
        | RemoteError::Discovery(_)
        | RemoteError::Randomness(_) => {
            tr!("the network connection failed")
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::sync::Arc;

    use super::*;

    const FAILURES: [RemoteFailure; 6] = [
        RemoteFailure::DifferentServer,
        RemoteFailure::NotConnected,
        RemoteFailure::NothingToTransfer,
        RemoteFailure::VersionMismatch,
        RemoteFailure::Busy,
        RemoteFailure::Stopped,
    ];

    #[test]
    fn given_each_failure_when_described_then_every_cause_reads_differently() {
        let texts: HashSet<String> = FAILURES.into_iter().map(failure_text).collect();

        assert_eq!(texts.len(), FAILURES.len());
    }

    #[test]
    fn given_a_failure_from_the_other_device_when_described_then_it_reads_as_that_failure() {
        for failure in FAILURES {
            assert_eq!(
                error_text(&RemoteError::Failed(failure)),
                failure_text(failure)
            );
        }
    }

    #[test]
    fn given_an_unreachable_device_when_described_then_the_firewall_port_is_named() {
        assert!(error_text(&RemoteError::Unreachable).contains(&PREFERRED_PORT.to_string()));
    }

    #[test]
    fn given_connection_errors_when_described_then_each_kind_has_its_text() {
        let network = error_text(&RemoteError::Io(Arc::new(std::io::Error::from(
            std::io::ErrorKind::ConnectionReset,
        ))));
        let texts: HashSet<String> = [
            error_text(&RemoteError::Denied),
            error_text(&RemoteError::TimedOut),
            error_text(&RemoteError::Unreachable),
            error_text(&RemoteError::Protocol("expected a hello")),
            network.clone(),
        ]
        .into_iter()
        .collect();

        assert_eq!(texts.len(), 4);
        assert_eq!(
            error_text(&RemoteError::Protocol("expected a hello")),
            network
        );
    }
}
