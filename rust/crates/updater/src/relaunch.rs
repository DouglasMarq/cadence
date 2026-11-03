#[cfg(any(windows, test))]
use std::ffi::OsStr;
use std::ffi::OsString;
use std::io;
#[cfg(windows)]
use std::os::windows::process::CommandExt as _;
use std::process::{Command, Stdio};

#[cfg(unix)]
const SHELL: &str = "/bin/sh";
#[cfg(unix)]
const ROOT_DIRECTORY: &str = "/";
#[cfg(unix)]
const WAITER_NAME: &str = "cadence-relaunch";
#[cfg(unix)]
const WAIT_THEN_LAUNCH: &str = r#"
attempts=0
while kill -0 "$1" 2>/dev/null && [ "$attempts" -lt 100 ]; do
    attempts=$((attempts + 1))
    sleep 0.1
done
shift
exec "$@"
"#;
#[cfg(windows)]
const POWERSHELL: &str = "powershell.exe";
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
#[cfg(any(windows, test))]
const WAIT_SECONDS: u32 = 10;
#[cfg(any(windows, test))]
const POWERSHELL_SINGLE_QUOTES: [char; 5] = ['\'', '\u{2018}', '\u{2019}', '\u{201A}', '\u{201B}'];

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

    #[cfg(unix)]
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

    #[cfg(windows)]
    pub fn after_exit_of(&self, process_id: u32) -> io::Result<()> {
        Command::new(POWERSHELL)
            .args(["-NoProfile", "-NonInteractive", "-Command"])
            .arg(self.windows_waiter(process_id))
            .creation_flags(CREATE_NO_WINDOW)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map(drop)
    }

    #[cfg(any(windows, test))]
    fn windows_waiter(&self, process_id: u32) -> String {
        let launch: Vec<String> = std::iter::once(&self.program)
            .chain(&self.arguments)
            .map(|value| powershell_literal(value))
            .collect();
        format!(
            "Wait-Process -Id {process_id} -Timeout {WAIT_SECONDS} -ErrorAction SilentlyContinue; & {}",
            launch.join(" ")
        )
    }
}

#[cfg(any(windows, test))]
fn powershell_literal(value: &OsStr) -> String {
    let escaped: String = value
        .to_string_lossy()
        .chars()
        .flat_map(|character| std::iter::repeat_n(character, copies_in_literal(character)))
        .collect();
    format!("'{escaped}'")
}

#[cfg(any(windows, test))]
fn copies_in_literal(character: char) -> usize {
    match POWERSHELL_SINGLE_QUOTES.contains(&character) {
        true => 2,
        false => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_an_installed_app_when_writing_the_windows_waiter_then_it_waits_then_starts_the_app() {
        let relaunch = Relaunch::new(r"C:\Apps\Cadence\cadence.exe", []);

        assert_eq!(
            relaunch.windows_waiter(4242),
            "Wait-Process -Id 4242 -Timeout 10 -ErrorAction SilentlyContinue; \
             & 'C:\\Apps\\Cadence\\cadence.exe'"
        );
    }

    #[test]
    fn given_quotes_in_the_path_and_arguments_when_writing_the_windows_waiter_then_each_stays_one_literal()
     {
        let relaunch = Relaunch::new(
            r"C:\Users\Ana O'Neil\Cadence\cadence.exe",
            [OsString::from("--name=\u{2018}Beta\u{2019}")],
        );

        assert!(
            relaunch.windows_waiter(1).ends_with(
                "& 'C:\\Users\\Ana O''Neil\\Cadence\\cadence.exe' \
                 '--name=\u{2018}\u{2018}Beta\u{2019}\u{2019}'"
            ),
            "{}",
            relaunch.windows_waiter(1)
        );
    }
}
