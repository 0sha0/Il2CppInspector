//! One CLI run: the stages and final status shown in the task list and details column, and its output for the copied log.

use std::path::Path;
use std::process::ExitStatus;
use std::time::{Duration, Instant};

use crate::cli::{Event, Process, Progress, Request};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

impl Status {
    pub fn text(self) -> &'static str {
        match self {
            Self::Running => "Running",
            Self::Succeeded => "Succeeded",
            Self::Failed => "Failed",
            Self::Cancelled => "Cancelled",
        }
    }
}

pub struct Stage {
    pub progress: Progress,
    started: Instant,
    finished: Option<Instant>,
}

impl Stage {
    /// The CLI `ProgressBar` reports a finished operation once more with `Current == Total` and the detail `done`,
    /// or `failed` when the operation threw.
    pub fn complete(&self) -> bool {
        self.progress.total >= 0 && self.progress.current >= self.progress.total
    }

    pub fn failed(&self) -> bool {
        self.progress.detail.as_deref() == Some("failed")
    }

    pub fn ended(&self) -> bool {
        self.complete() || self.failed()
    }

    /// Null while the total is unknown.
    pub fn ratio(&self) -> Option<f32> {
        match self.progress.total {
            total if total < 0 => None,
            0 => Some(1.),
            total => Some((self.progress.current as f64 / total as f64).clamp(0., 1.) as f32),
        }
    }

    pub fn elapsed(&self) -> Duration {
        self.finished.unwrap_or_else(Instant::now) - self.started
    }

    /// Percentage, then the elapsed time. The CLI counts internal work units (several stages report thousandths), so
    /// the raw counts mean nothing to the user.
    pub fn summary(&self) -> String {
        let elapsed = format!("{:.1} s", self.elapsed().as_secs_f32());
        match self.ratio() {
            Some(ratio) => format!("{}% · {elapsed}", (ratio * 100.).floor()),
            None => elapsed,
        }
    }

    /// What the CLI is working on, such as an assembly name; the completion markers are shown by the bar instead.
    pub fn detail(&self) -> Option<&str> {
        self.progress
            .detail
            .as_deref()
            .filter(|detail| !detail.is_empty() && !matches!(*detail, "done" | "failed"))
    }
}

pub struct Job {
    pub id: u64,
    pub request: Request,
    pub status: Status,
    pub stages: Vec<Stage>,
    /// Status and other output lines; only the copied log includes them.
    pub lines: Vec<String>,
    pub errors: Vec<String>,
    /// Set when the process could not be waited for or exited without writing an error.
    pub failure: Option<String>,
    process: Option<Process>,
    cancelling: bool,
}

impl Job {
    pub fn new(id: u64, request: Request, process: Process) -> Self {
        Self {
            id,
            request,
            status: Status::Running,
            stages: Vec::new(),
            lines: Vec::new(),
            errors: Vec::new(),
            failure: None,
            process: Some(process),
            cancelling: false,
        }
    }

    /// The selected game, or the output folder name when the plugin is detected automatically.
    pub fn title(&self) -> String {
        self.request
            .game
            .clone()
            .or_else(|| self.request.output.file_name().map(|name| name.to_string_lossy().into_owned()))
            .unwrap_or_else(|| self.request.output.display().to_string())
    }

    pub fn targets(&self) -> String {
        if self.request.targets.is_empty() {
            "DummyDll".to_owned()
        } else {
            self.request.targets.join(" / ")
        }
    }

    /// The stage the CLI is working on, if it has not finished yet.
    pub fn active_stage(&self) -> Option<&Stage> {
        self.stages.last().filter(|stage| !stage.ended())
    }

    pub fn apply(&mut self, event: Event) {
        match event {
            Event::Progress(progress) => record(&mut self.stages, progress, Instant::now()),
            Event::Status(line) | Event::Line(line) => self.lines.push(line),
            Event::Error(line) => self.errors.push(line),
            Event::Exited(status) => self.exit(status),
        }
    }

    fn exit(&mut self, status: Result<ExitStatus, String>) {
        self.process = None;
        if let Some(stage) = self.stages.last_mut() {
            stage.finished.get_or_insert_with(Instant::now);
        }
        self.status = match status {
            _ if self.cancelling => Status::Cancelled,
            Ok(status) if status.success() => Status::Succeeded,
            Ok(status) => {
                if self.errors.is_empty() {
                    self.failure = Some(match status.code() {
                        Some(code) => format!("Il2CppInspector exited with code {code}."),
                        None => "Il2CppInspector was terminated.".to_owned(),
                    });
                }
                Status::Failed
            }
            Err(error) => {
                self.failure = Some(format!("Cannot wait for Il2CppInspector: {error}"));
                Status::Failed
            }
        };
    }

    pub fn cancel(&mut self) {
        if let Some(process) = &self.process {
            self.cancelling = true;
            process.kill();
        }
    }
}

/// The CLI writes one stage at a time and updates it in place, so a new label or a repeated label after the
/// previous stage ended starts the next stage.
fn record(stages: &mut Vec<Stage>, progress: Progress, now: Instant) {
    match stages.last_mut() {
        Some(stage) if stage.progress.label == progress.label && !stage.ended() => stage.progress = progress,
        previous => {
            if let Some(stage) = previous {
                stage.finished.get_or_insert(now);
            }
            stages.push(Stage {
                progress,
                started: now,
                finished: None,
            });
        }
    }
    if let Some(stage) = stages.last_mut().filter(|stage| stage.ended()) {
        stage.finished.get_or_insert(now);
    }
}

/// Two jobs writing into the same tree would overwrite each other's files.
pub fn outputs_overlap(first: &Path, second: &Path) -> bool {
    first.starts_with(second) || second.starts_with(first)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn progress(label: &str, current: i64, total: i64, detail: Option<&str>) -> Progress {
        Progress {
            label: label.into(),
            current,
            total,
            detail: detail.map(str::to_owned),
        }
    }

    #[test]
    fn summarizes_progress_as_a_floored_percentage() {
        let stage = |current, total| Stage {
            progress: progress("Generating C++ types", current, total, None),
            started: Instant::now(),
            finished: Some(Instant::now()),
        };
        assert_eq!(stage(999, 1000).summary(), "99% · 0.0 s");
        assert_eq!(stage(1000, 1000).summary(), "100% · 0.0 s");
        assert_eq!(stage(0, -1).summary(), "0.0 s");
    }

    #[test]
    fn updates_stages_in_place_until_they_end() {
        let start = Instant::now();
        let mut stages = Vec::new();
        for (offset, progress) in [
            progress("Building type model", 0, -1, None),
            progress("Building type model", 5, 10, None),
            progress("Building type model", 10, 10, Some("done")),
            progress("Generating DummyDlls", 1, 4, None),
            progress("Generating DummyDlls", 1, 4, Some("failed")),
            progress("Generating DummyDlls", 0, -1, None),
        ]
        .into_iter()
        .enumerate()
        {
            record(&mut stages, progress, start + Duration::from_secs(offset as u64));
        }
        let summary: Vec<_> = stages
            .iter()
            .map(|stage| {
                (
                    stage.progress.label.as_str(),
                    stage.progress.current,
                    stage.ended(),
                    stage.finished.map(|end| end - start),
                )
            })
            .collect();
        assert_eq!(
            summary,
            [
                ("Building type model", 10, true, Some(Duration::from_secs(2))),
                ("Generating DummyDlls", 1, true, Some(Duration::from_secs(4))),
                ("Generating DummyDlls", 0, false, None),
            ]
        );
    }

    #[test]
    fn detects_overlapping_outputs() {
        assert!(outputs_overlap(Path::new("/out"), Path::new("/out/zzz")));
        assert!(outputs_overlap(Path::new("/out/zzz"), Path::new("/out/zzz")));
        assert!(!outputs_overlap(Path::new("/out/zzz"), Path::new("/out/zz")));
    }
}
