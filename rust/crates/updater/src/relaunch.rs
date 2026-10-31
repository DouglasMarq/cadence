use std::ffi::OsString;
use std::io;
use std::process::{Command, Stdio};

const SHELL: &str = "/bin/sh";
const ROOT_DIRECTORY: &str = "/";
const WAITER_NAME: &str = "cadence-relaunch";
const WAIT_THEN_LAUNCH: &str = r#"
attempts=0
while kill -0 "$1" 2>/dev/null && [ "$attempts" -lt 100 ]; do
    attempts=$((attempts + 1))
    sleep 0.1
done
shift
exec "$@"
"#;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Relaunch {
    program: OsString,
    arguments: Vec<OsString>,
}

impl Relaunch {
    pub fn new(
        program: impl Into<OsString>,
        arguments: impl IntoIterator<Item = OsString>,
    ) -> Self {
        Self {
            program: program.into(),
            arguments: arguments.into_iter().collect(),
        }
    }

    pub fn after_exit_of(&self, process_id: u32) -> io::Result<()> {
        Command::new(SHELL)
            .arg("-c")
            .arg(WAIT_THEN_LAUNCH)
            .arg(WAITER_NAME)
            .arg(process_id.to_string())
            .arg(&self.program)
            .args(&self.arguments)
            .current_dir(ROOT_DIRECTORY)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map(drop)
    }
}
