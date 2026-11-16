//! Runs the Il2CppInspector CLI as a child process and decodes its `--json-progress` output.

use std::ffi::OsString;
use std::io::{self, BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;

use serde::Deserialize;

#[cfg(windows)]
const EXECUTABLE: &str = "Il2CppInspector.exe";
#[cfg(not(windows))]
const EXECUTABLE: &str = "Il2CppInspector";

/// Output targets accepted by the CLI `-t` option.
pub const TARGETS: [&str; 4] = ["IDA", "Ghidra", "BinaryNinja", "Debug"];

/// Shown when [`locate`] finds no CLI.
pub const NOT_FOUND: &str =
    "Il2CppInspector CLI not found. Build Il2CppInspector.CLI, place this program beside Il2CppInspector, or set IL2CPP_INSPECTOR_CLI.";

/// Finds the CLI: `IL2CPP_INSPECTOR_CLI`, then beside this executable, then the Release or Debug build of a source
/// checkout above it (`Il2CppInspector.GUI/target/<profile>/` next to `Il2CppInspector.CLI/bin/<configuration>/net10.0/`).
pub fn locate() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("IL2CPP_INSPECTOR_CLI") {
        return Some(PathBuf::from(path));
    }
    let executable = std::env::current_exe().ok()?;
    let directory = executable.parent()?;
    let beside = directory.join(EXECUTABLE);
    if beside.is_file() {
        return Some(beside);
    }
    directory.ancestors().find_map(|ancestor| {
        ["Release", "Debug"]
            .iter()
            .map(|configuration| {
                ancestor
                    .join("Il2CppInspector.CLI")
                    .join("bin")
                    .join(configuration)
                    .join("net10.0")
                    .join(EXECUTABLE)
            })
            .find(|path| path.is_file())
    })
}

/// Runs `--list-games` and returns the installed plugin IDs.
pub fn list_games(cli: &Path) -> Result<Vec<String>, String> {
    let output = command(cli)
        .arg("--list-games")
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("Cannot run {}: {error}", cli.display()))?;
    if !output.status.success() {
        return Err(format!(
            "{} --list-games failed: {}",
            cli.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect())
}

/// One CLI invocation; optional values are omitted from the command line when `None`.
#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    pub binary: PathBuf,
    pub metadata: PathBuf,
    pub output: PathBuf,
    pub game: Option<String>,
    pub targets: Vec<&'static str>,
    pub startup_metadata: Option<PathBuf>,
    pub unity_version: Option<String>,
    pub image_base: Option<String>,
}

impl Request {
    pub fn arguments(&self) -> Vec<OsString> {
        let mut arguments: Vec<OsString> = vec![
            "--json-progress".into(),
            "-i".into(),
            self.binary.clone().into(),
            "-m".into(),
            self.metadata.clone().into(),
            "-o".into(),
            self.output.clone().into(),
        ];
        for target in &self.targets {
            arguments.extend(["-t".into(), (*target).into()]);
        }
        if let Some(game) = &self.game {
            arguments.extend(["--game".into(), game.into()]);
        }
        if let Some(path) = &self.startup_metadata {
            arguments.extend(["--startup-metadata".into(), path.clone().into()]);
        }
        if let Some(version) = &self.unity_version {
            arguments.extend(["--unity-version".into(), version.into()]);
        }
        if let Some(base) = &self.image_base {
            arguments.extend(["--image-base".into(), base.into()]);
        }
        arguments
    }
}

/// Progress of one CLI `ProgressBar` operation; `total` is negative while the amount of work is unknown.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Progress {
    pub label: String,
    pub current: i64,
    pub total: i64,
    pub detail: Option<String>,
}

#[derive(Debug, PartialEq)]
pub enum Event {
    Progress(Progress),
    /// A `ProgressBar.WriteStatus` message.
    Status(String),
    /// Any other standard output line.
    Line(String),
    /// A standard error line.
    Error(String),
    Exited(Result<ExitStatus, String>),
}

#[derive(Deserialize)]
#[serde(tag = "event", rename_all = "lowercase")]
enum Message {
    Progress(Progress),
    Status { message: String },
}

/// Decodes one standard output line written with `--json-progress`; lines that are not progress objects stay text.
pub fn parse(line: &str) -> Event {
    match serde_json::from_str(line) {
        Ok(Message::Progress(progress)) => Event::Progress(progress),
        Ok(Message::Status { message }) => Event::Status(message),
        Err(_) => Event::Line(line.to_owned()),
    }
}

/// A running CLI process. Its events arrive on the channel given to [`spawn`], ending with [`Event::Exited`].
pub struct Process {
    child: Arc<Mutex<Child>>,
}

impl Process {
    /// Stops the process. The waiting thread holds the lock only after the output pipes closed, when the process is
    /// already exiting, so this never blocks the UI thread.
    pub fn kill(&self) {
        if let Ok(mut child) = self.child.try_lock() {
            let _ = child.kill();
        }
    }
}

pub fn spawn<K: Copy + Send + 'static>(cli: &Path, request: &Request, key: K, sender: mpsc::Sender<(K, Event)>) -> io::Result<Process> {
    let mut child = command(cli)
        .args(request.arguments())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let child = Arc::new(Mutex::new(child));
    let waiter = child.clone();
    let errors = sender.clone();
    let stderr_reader = thread::spawn(move || {
        if let Some(stderr) = stderr {
            for line in lines(stderr) {
                let _ = errors.send((key, Event::Error(line)));
            }
        }
    });
    thread::spawn(move || {
        if let Some(stdout) = stdout {
            for line in lines(stdout) {
                let _ = sender.send((key, parse(&line)));
            }
        }
        let _ = stderr_reader.join();
        let status = match waiter.lock() {
            Ok(mut child) => child.wait().map_err(|error| error.to_string()),
            Err(_) => Err("The process state is unavailable.".to_owned()),
        };
        let _ = sender.send((key, Event::Exited(status)));
    });
    Ok(Process { child })
}

fn command(cli: &Path) -> Command {
    let mut command = Command::new(cli);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW: the GUI has no console, so the CLI would otherwise open one.
        command.creation_flags(0x0800_0000);
    }
    command
}

/// Splits a UTF-8 stream into lines without failing on invalid bytes; .NET on Windows ends lines with CRLF.
fn lines(stream: impl Read) -> impl Iterator<Item = String> {
    BufReader::new(stream).split(b'\n').map_while(Result::ok).filter_map(|bytes| {
        let line = String::from_utf8_lossy(&bytes);
        let line = line.trim_end_matches('\r');
        (!line.is_empty()).then(|| line.to_owned())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_cli_progress_objects() {
        assert_eq!(
            parse(r#"{"event":"progress","label":"Generating DummyDlls","current":3,"total":10,"detail":"mscorlib"}"#),
            Event::Progress(Progress {
                label: "Generating DummyDlls".into(),
                current: 3,
                total: 10,
                detail: Some("mscorlib".into()),
            })
        );
        assert_eq!(
            parse(r#"{"event":"progress","label":"Building application model","current":0,"total":-1,"detail":null}"#),
            Event::Progress(Progress {
                label: "Building application model".into(),
                current: 0,
                total: -1,
                detail: None,
            })
        );
        assert_eq!(
            parse(r#"{"event":"status","message":"Processed 2000 relocations"}"#),
            Event::Status("Processed 2000 relocations".into())
        );
    }

    #[test]
    fn keeps_other_output_as_text() {
        assert_eq!(parse("Loading IL2CPP data..."), Event::Line("Loading IL2CPP data...".into()));
        assert_eq!(parse(r#"{"event":"unknown"}"#), Event::Line(r#"{"event":"unknown"}"#.into()));
    }

    #[test]
    fn omits_unset_options() {
        let request = Request {
            binary: "GameAssembly.dll".into(),
            metadata: "global-metadata.dat".into(),
            output: "out".into(),
            game: Some("ZZZ_CN_Windows_3.3.0".into()),
            targets: vec!["IDA", "Debug"],
            startup_metadata: None,
            unity_version: None,
            image_base: Some("7f000000".into()),
        };
        assert_eq!(
            request.arguments(),
            [
                "--json-progress",
                "-i",
                "GameAssembly.dll",
                "-m",
                "global-metadata.dat",
                "-o",
                "out",
                "-t",
                "IDA",
                "-t",
                "Debug",
                "--game",
                "ZZZ_CN_Windows_3.3.0",
                "--image-base",
                "7f000000",
            ]
            .map(OsString::from)
        );
    }

    #[test]
    fn splits_crlf_lines_and_skips_blank_ones() {
        let output: Vec<_> = lines(&b"Loading\r\n\r\nDone.\r\n\xff\n"[..]).collect();
        assert_eq!(output, ["Loading", "Done.", "\u{fffd}"]);
    }
}
