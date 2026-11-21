//! The window: launcher, task list and task details side by side, with a movable settings window above them.

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use gpui::{prelude::*, *};

use crate::cli::{self, Event, Request, TARGETS};
use crate::input::TextInput;
use crate::job::{self, Job, Stage, Status};
use crate::picker::GamePicker;
use crate::ui::{self, Fill, Tone};

actions!(workspace, [CloseSettings]);

const CONTEXT: &str = "Workspace";
const LAUNCHER_WIDTH: f32 = 640.;
const TASKS_WIDTH: f32 = 280.;
const SETTINGS_WIDTH: f32 = 560.;
const SETTINGS_HEADER: f32 = 42.;

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([KeyBinding::new("escape", CloseSettings, Some(CONTEXT))]);
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Field {
    Binary,
    Metadata,
    Output,
    StartupMetadata,
}

/// Drag payload of the settings window header.
struct SettingsDrag;

pub struct Workspace {
    focus_handle: FocusHandle,
    cli: Option<PathBuf>,
    picker: Entity<GamePicker>,
    binary: Entity<TextInput>,
    metadata: Entity<TextInput>,
    output: Entity<TextInput>,
    startup_metadata: Entity<TextInput>,
    unity_version: Entity<TextInput>,
    image_base: Entity<TextInput>,
    targets: [bool; TARGETS.len()],
    settings_open: bool,
    /// Top-left corner after the user moved the settings window; it starts centered.
    settings_position: Option<Point<Pixels>>,
    drag_offset: Point<Pixels>,
    picking: bool,
    error: Option<String>,
    jobs: Vec<Job>,
    selected: Option<u64>,
    next_id: u64,
    sender: mpsc::Sender<(u64, Event)>,
    tasks_scroll: ScrollHandle,
    details_scroll: ScrollHandle,
    _pump: Task<()>,
    _quit: Subscription,
    _picker_changes: Subscription,
}

impl Workspace {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (sender, receiver) = mpsc::channel();
        // Job threads cannot touch the view; their events are collected at about 30 frames per second.
        let pump = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_millis(33)).await;
                let events: Vec<_> = receiver.try_iter().collect();
                if this.update(cx, |this, cx| this.receive(events, cx)).is_err() {
                    break;
                }
            }
        });
        // Child processes outlive the GUI on Windows unless they are stopped explicitly.
        let quit = cx.on_app_quit(|this, _| {
            for job in &mut this.jobs {
                job.cancel();
            }
            async {}
        });
        let focus_handle = cx.focus_handle();
        window.focus(&focus_handle);
        let cli = cli::locate();
        let picker = cx.new(|cx| GamePicker::new(cli.clone(), cx));
        let picker_changes = cx.observe_in(&picker, window, |this, _, window, cx| {
            for field in [Field::Binary, Field::Metadata, Field::StartupMetadata] {
                let enabled = this.input_enabled(field, cx);
                this.input(field).update(cx, |input, cx| input.set_enabled(enabled, window, cx));
            }
            cx.notify();
        });
        Self {
            focus_handle,
            picker,
            cli,
            binary: cx.new(|cx| TextInput::new("GameAssembly.dll or libil2cpp.so", cx)),
            metadata: cx.new(|cx| TextInput::new("global-metadata.dat", cx)),
            output: cx.new(|cx| TextInput::new("Output directory", cx)),
            startup_metadata: cx.new(|cx| TextInput::new("Detected beside the metadata file", cx)),
            unity_version: cx.new(|cx| TextInput::new("Detected, e.g. 2021.3.0f1", cx)),
            image_base: cx.new(|cx| TextInput::new("Hex, for ELF memory dumps", cx)),
            targets: [false; TARGETS.len()],
            settings_open: false,
            settings_position: None,
            drag_offset: Point::default(),
            picking: false,
            error: None,
            jobs: Vec::new(),
            selected: None,
            next_id: 1,
            sender,
            tasks_scroll: ScrollHandle::new(),
            details_scroll: ScrollHandle::new(),
            _pump: pump,
            _quit: quit,
            _picker_changes: picker_changes,
        }
    }

    fn receive(&mut self, events: Vec<(u64, Event)>, cx: &mut Context<Self>) {
        // Running stages show their elapsed time, so redraw while any job runs even without new events.
        if events.is_empty() && !self.jobs.iter().any(|job| job.status == Status::Running) {
            return;
        }
        // Follow new output at the bottom of the details; scrolling up pauses following until it is back at the bottom.
        let selected = self.selected;
        let following = self.details_scroll.max_offset().height + self.details_scroll.offset().y < px(32.);
        let mut selected_changed = false;
        for (id, event) in events {
            if let Some(job) = self.jobs.iter_mut().find(|job| job.id == id) {
                job.apply(event);
                selected_changed |= selected == Some(id);
            }
        }
        if following && selected_changed {
            self.details_scroll.scroll_to_bottom();
        }
        cx.notify();
    }

    fn select(&mut self, id: u64, cx: &mut Context<Self>) {
        self.selected = Some(id);
        self.details_scroll.scroll_to_bottom();
        cx.notify();
    }

    fn start(&mut self, cx: &mut Context<Self>) {
        self.error = None;
        match self.request(cx).and_then(|request| self.spawn(request)) {
            Ok(id) => self.select(id, cx),
            Err(error) => {
                self.error = Some(error);
                cx.notify();
            }
        }
    }

    fn request(&self, cx: &App) -> Result<Request, String> {
        let game = self.picker.read(cx).game()?;
        let inputs = self.picker.read(cx).inputs();
        let binary = self.input_path(Field::Binary, "Binary", inputs.is_none_or(|inputs| inputs.binary), cx)?;
        let metadata = self.input_path(Field::Metadata, "Metadata", inputs.is_some_and(|inputs| inputs.metadata), cx)?;
        let startup_metadata = self.input_path(Field::StartupMetadata, "Startup metadata", false, cx)?;
        let output = path(&self.output, cx).ok_or("An output directory is required.")?;
        Ok(Request {
            binary,
            metadata,
            output,
            game,
            targets: TARGETS
                .iter()
                .zip(self.targets)
                .filter(|(_, on)| *on)
                .map(|(target, _)| *target)
                .collect(),
            startup_metadata,
            unity_version: text(&self.unity_version, cx),
            image_base: text(&self.image_base, cx),
        })
    }

    fn input_path(&self, field: Field, label: &str, required: bool, cx: &App) -> Result<Option<PathBuf>, String> {
        if !self.input_enabled(field, cx) {
            return Ok(None);
        }
        let value = path(self.input(field), cx);
        if required && value.is_none() {
            return Err(format!("{label} file is required."));
        }
        if let Some(path) = value.as_deref().filter(|path| !path.is_file()) {
            return Err(format!("{label} file not found: {}", path.display()));
        }
        Ok(value)
    }

    fn input_enabled(&self, field: Field, cx: &App) -> bool {
        self.picker.read(cx).inputs().is_none_or(|inputs| match field {
            Field::Binary => inputs.binary,
            Field::Metadata => inputs.metadata,
            Field::StartupMetadata => inputs.startup_metadata,
            Field::Output => true,
        })
    }

    fn spawn(&mut self, request: Request) -> Result<u64, String> {
        let cli = self.cli.as_deref().ok_or(cli::NOT_FOUND)?;
        if let Some(other) = self
            .jobs
            .iter()
            .find(|job| job.status == Status::Running && job::outputs_overlap(&request.output, &job.request.output))
        {
            return Err(format!(
                "Output directory {} overlaps running task {} ({}).",
                request.output.display(),
                other.title(),
                other.request.output.display()
            ));
        }
        let id = self.next_id;
        let process = cli::spawn(cli, &request, id, self.sender.clone()).map_err(|error| format!("Cannot start {}: {error}", cli.display()))?;
        self.next_id += 1;
        self.jobs.push(Job::new(id, request, process));
        Ok(id)
    }

    /// Cancels a running job, or removes a finished one from the list.
    fn remove(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(index) = self.jobs.iter().position(|job| job.id == id) else {
            return;
        };
        if self.jobs[index].status == Status::Running {
            self.jobs[index].cancel();
        } else {
            self.jobs.remove(index);
            if self.selected == Some(id) {
                self.selected = None;
            }
        }
        cx.notify();
    }

    fn input(&self, field: Field) -> &Entity<TextInput> {
        match field {
            Field::Binary => &self.binary,
            Field::Metadata => &self.metadata,
            Field::Output => &self.output,
            Field::StartupMetadata => &self.startup_metadata,
        }
    }

    fn pick(&mut self, field: Field, cx: &mut Context<Self>) {
        if self.picking || !self.input_enabled(field, cx) {
            return;
        }
        let directories = field == Field::Output;
        self.picking = true;
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: !directories,
            directories,
            multiple: false,
            prompt: None,
        });
        cx.spawn(async move |this, cx| {
            let result = paths.await;
            let _ = this.update(cx, |this, cx| {
                this.picking = false;
                match result {
                    Ok(Ok(Some(paths))) => {
                        if let Some(path) = paths.into_iter().next() {
                            this.set_path(field, &path, cx);
                        }
                    }
                    Ok(Err(error)) => this.error = Some(format!("Cannot open the file dialog: {error}")),
                    // The dialog was cancelled or closed with the window.
                    Ok(Ok(None)) | Err(_) => {}
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn set_path(&mut self, field: Field, path: &Path, cx: &mut Context<Self>) {
        if !self.input_enabled(field, cx) {
            return;
        }
        self.input(field).update(cx, |input, cx| input.set_text(path.display().to_string(), cx));
        if field == Field::Binary
            && self.input_enabled(Field::Metadata, cx)
            && self.metadata.read(cx).text().is_empty()
            && let Some(metadata) = find_metadata(path)
        {
            self.metadata.update(cx, |input, cx| input.set_text(metadata.display().to_string(), cx));
        }
    }

    fn copy_log(&self, id: u64, cx: &mut Context<Self>) {
        let Some(job) = self.jobs.iter().find(|job| job.id == id) else {
            return;
        };
        let mut log: Vec<String> = details(job).into_iter().map(|(label, value)| format!("{label}: {value}")).collect();
        log.extend(job.stages.iter().map(|stage| format!("{} [{}]", stage.progress.label, stage.summary())));
        log.extend(job.lines.iter().cloned());
        log.extend(job.errors.iter().cloned());
        log.extend(job.failure.iter().cloned());
        log.push(job.status.text().to_owned());
        cx.write_to_clipboard(ClipboardItem::new_string(log.join("\n")));
    }

    fn drag_settings(&mut self, event: &DragMoveEvent<SettingsDrag>, window: &mut Window, cx: &mut Context<Self>) {
        // The header starts one border width inside the window, and the header stays reachable inside the viewport.
        let viewport = window.viewport_size();
        let origin = event.event.position - self.drag_offset - point(px(1.), px(1.));
        let max_x = (viewport.width - settings_width(viewport)).max(px(0.));
        let max_y = (viewport.height - px(SETTINGS_HEADER)).max(px(0.));
        self.settings_position = Some(point(origin.x.clamp(px(0.), max_x), origin.y.clamp(px(0.), max_y)));
        cx.notify();
    }

    fn launcher(&self, cx: &mut Context<Self>) -> Div {
        let start_enabled = self.cli.is_some();
        let actions = div()
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap(px(8.))
            .child(ui::segments(TARGETS.iter().enumerate().map(|(index, target)| {
                ui::segment(
                    ("target", index),
                    *target,
                    self.targets[index],
                    cx.listener(move |this, _, _, cx| {
                        this.targets[index] = !this.targets[index];
                        cx.notify();
                    }),
                )
            })))
            .child(ui::button(
                "settings",
                "Settings",
                Tone::Normal,
                true,
                cx.listener(|this, _, _, cx| {
                    this.settings_open = !this.settings_open;
                    cx.notify();
                }),
            ))
            .child(ui::button(
                "start",
                "Start",
                Tone::Primary,
                start_enabled,
                cx.listener(|this, _, _, cx| this.start(cx)),
            ));
        div()
            .w(px(LAUNCHER_WIDTH))
            .flex_shrink_0()
            .h_full()
            .flex()
            .flex_col()
            .border_r_1()
            .border_color(rgba(ui::SEPARATOR))
            .bg(rgba(ui::LAUNCHER_TINT))
            .child(
                // Only the game table gives up height; the fields and Start stay visible until it reaches its minimum.
                div()
                    .id("launcher")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap(px(14.))
                    .pt(px(16.))
                    .px(px(16.))
                    .pb(px(24.))
                    .child(self.picker.clone())
                    .child(self.path_field("Binary", Field::Binary, cx))
                    .child(self.path_field("Metadata", Field::Metadata, cx))
                    .child(self.path_field("Output", Field::Output, cx))
                    .child(actions)
                    .when_some(self.error.clone(), |form, error| {
                        form.child(div().flex_shrink_0().text_color(rgba(ui::DANGER_TEXT)).child(error))
                    }),
            )
    }

    fn path_field(&self, label: &'static str, field: Field, cx: &mut Context<Self>) -> Div {
        labeled(
            label,
            div().flex().gap(px(6.)).child(self.input(field).clone()).child(ui::button(
                ("pick", field as usize),
                "Select",
                Tone::Normal,
                !self.picking && self.input_enabled(field, cx),
                cx.listener(move |this, _, _, cx| this.pick(field, cx)),
            )),
        )
    }

    fn tasks(&self, cx: &mut Context<Self>) -> Div {
        let list = div()
            .flex()
            .flex_col()
            .pt(px(10.))
            .px(px(6.))
            .pb(px(24.))
            .when(self.jobs.is_empty(), |list| list.child(ui::message("No tasks")))
            .children(self.jobs.iter().map(|job| self.task(job, cx)));
        div()
            .w(px(TASKS_WIDTH))
            .flex_shrink_0()
            .h_full()
            .flex()
            .flex_col()
            .border_r_1()
            .border_color(rgba(ui::SEPARATOR))
            .child(ui::scroll_area("tasks", &self.tasks_scroll, true, list))
    }

    fn task(&self, job: &Job, cx: &mut Context<Self>) -> Stateful<Div> {
        let selected = self.selected == Some(job.id);
        let id = job.id;
        let stage = job.active_stage().filter(|_| job.status == Status::Running);
        let state = match job.status {
            Status::Running => stage.map_or("Starting".into(), |stage| stage.progress.label.clone()),
            status => status.text().to_owned(),
        };
        // The targets follow the state on one line so that a long list is cut off first and leaves the title its width.
        let meta = format!("{state} · {}", job.targets());
        let muted = if selected { ui::ON_ACCENT_SECONDARY } else { ui::TERTIARY };
        div()
            .id(("task", id))
            .flex()
            .flex_col()
            .gap(px(4.))
            .py(px(8.))
            .px(px(10.))
            .rounded(px(7.))
            .map(|task| {
                if selected {
                    task.bg(rgba(ui::ACCENT)).text_color(rgba(ui::WHITE))
                } else {
                    task.hover(|style| style.bg(rgba(ui::ROW_HOVER)))
                }
            })
            .on_click(cx.listener(move |this, _, _, cx| this.select(id, cx)))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .min_w_0()
                    .child(ui::dot(status_color(job.status)))
                    .child(div().flex_1().min_w_0().truncate().child(job.title())),
            )
            .child(div().pl(px(16.)).text_size(px(11.)).text_color(rgba(muted)).truncate().child(meta))
            .when_some(stage, |task, stage| {
                let (fill, track) = if selected {
                    (Fill::White, 0xffffff40)
                } else {
                    (Fill::Accent, 0xffffff1a)
                };
                task.child(div().pl(px(16.)).child(ui::progress(("task-progress", id), stage.ratio(), fill, track)))
            })
    }

    fn details(&self, cx: &mut Context<Self>) -> Div {
        let column = div().flex_1().min_w_0().h_full().flex().flex_col();
        let Some(job) = self.selected.and_then(|id| self.jobs.iter().find(|job| job.id == id)) else {
            return column.child(ui::message("Select a task to see its progress"));
        };
        let id = job.id;
        let running = job.status == Status::Running;
        let output = job.request.output.clone();
        let header = div()
            .h(px(52.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap(px(8.))
            .pl(px(16.))
            .pr(px(12.))
            .text_color(rgba(ui::SECONDARY))
            .text_size(px(12.))
            .font_weight(FontWeight::SEMIBOLD)
            .child(div().flex_1().min_w_0().truncate().child(format!("{} · {}", job.title(), job.targets())))
            .child(
                div()
                    .flex()
                    .gap(px(6.))
                    .child(ui::button(
                        "open-output",
                        "Open output",
                        Tone::Normal,
                        output.is_dir(),
                        move |_, _, cx| cx.open_with_system(&output),
                    ))
                    .child(ui::button(
                        "copy-log",
                        "Copy",
                        Tone::Normal,
                        true,
                        cx.listener(move |this, _, _, cx| this.copy_log(id, cx)),
                    ))
                    .child(ui::button(
                        "remove",
                        if running { "Cancel" } else { "Remove" },
                        if running { Tone::Danger } else { Tone::Normal },
                        true,
                        cx.listener(move |this, _, _, cx| this.remove(id, cx)),
                    )),
            );
        let paths = div()
            .flex()
            .flex_col()
            .gap(px(4.))
            .text_size(px(12.))
            .children(details(job).into_iter().map(|(label, value)| {
                div()
                    .flex()
                    .gap(px(12.))
                    .child(div().w(px(110.)).flex_shrink_0().text_color(rgba(ui::TERTIARY)).child(label))
                    .child(div().flex_1().min_w_0().child(value))
            }));
        let status = (!running).then(|| {
            let (color, text) = match job.status {
                Status::Succeeded => (ui::SUCCESS, Status::Succeeded.text().to_owned()),
                Status::Cancelled => (ui::WARNING, Status::Cancelled.text().to_owned()),
                status => (ui::DANGER_TEXT, job.failure.clone().unwrap_or_else(|| status.text().to_owned())),
            };
            div().font_weight(FontWeight::SEMIBOLD).text_color(rgba(color)).child(text)
        });
        let body = div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .px(px(16.))
            .pb(px(24.))
            .child(paths)
            .children(job.stages.iter().enumerate().map(|(index, stage)| stage_block(job, index, stage)))
            .children(status)
            .children(job.errors.iter().map(|line| div().text_color(rgba(ui::DANGER_TEXT)).child(line.clone())));
        column.child(header).child(ui::scroll_area("details", &self.details_scroll, true, body))
    }

    fn settings(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        let this = cx.entity().downgrade();
        let header = div()
            .id("settings-header")
            .h(px(SETTINGS_HEADER))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_between()
            .pl(px(16.))
            .pr(px(12.))
            .border_b_1()
            .border_color(rgba(ui::SEPARATOR))
            .bg(rgba(0xffffff08))
            .text_color(rgba(ui::SECONDARY))
            .text_size(px(12.))
            .font_weight(FontWeight::SEMIBOLD)
            .on_drag(SettingsDrag, move |_, offset, _, cx| {
                let _ = this.update(cx, |this, _| this.drag_offset = offset);
                cx.new(|_| EmptyView)
            })
            .child("Settings")
            .child(ui::button(
                "close-settings",
                "Close",
                Tone::Normal,
                true,
                cx.listener(|this, _, _, cx| {
                    this.settings_open = false;
                    cx.notify();
                }),
            ));
        let body = div()
            .flex()
            .flex_col()
            .gap(px(14.))
            .pt(px(16.))
            .px(px(18.))
            .pb(px(20.))
            .child(self.path_field("Startup metadata", Field::StartupMetadata, cx))
            .child(labeled("Unity version", div().flex().child(self.unity_version.clone())))
            .child(labeled("Image base", div().flex().child(self.image_base.clone())));
        let settings = div()
            .id("settings")
            .occlude()
            .w(settings_width(window.viewport_size()))
            .flex()
            .flex_col()
            .rounded(px(12.))
            .border_1()
            .border_color(rgba(ui::WINDOW_BORDER))
            .bg(rgba(ui::WINDOW))
            .shadow(vec![ui::window_shadow()])
            .overflow_hidden()
            .child(header)
            .child(body);
        // The full-size layer only positions the window; without listeners it lets clicks through to the columns.
        match self.settings_position {
            Some(position) => div().absolute().left(position.x).top(position.y).child(settings),
            None => div().absolute().inset_0().flex().items_center().justify_center().child(settings),
        }
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .key_context(CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|this, _: &CloseSettings, _, cx| {
                this.settings_open = false;
                cx.notify();
            }))
            .on_drag_move(cx.listener(Self::drag_settings))
            .relative()
            .size_full()
            .flex()
            .bg(linear_gradient(
                180.,
                linear_color_stop(rgba(ui::BACKGROUND_TOP), 0.),
                linear_color_stop(rgba(ui::BACKGROUND_BOTTOM), 1.),
            ))
            .font_family(ui::FONT)
            .text_size(px(13.))
            .line_height(px(18.))
            .text_color(rgba(ui::TEXT))
            .child(self.launcher(cx))
            .child(self.tasks(cx))
            .child(self.details(cx))
            .when(self.settings_open, |root| root.child(self.settings(window, cx)))
    }
}

fn stage_block(job: &Job, index: usize, stage: &Stage) -> Div {
    // A stage the CLI never closed stopped with the process: full bar in the cancel or failure color.
    let (ratio, fill) = if stage.failed() {
        (stage.ratio(), Fill::Danger)
    } else if stage.ended() || job.status == Status::Running {
        (stage.ratio(), Fill::Stage)
    } else if job.status == Status::Cancelled {
        (Some(1.), Fill::Warning)
    } else {
        (Some(1.), Fill::Danger)
    };
    div()
        .flex_shrink_0()
        .pt(px(10.))
        .px(px(12.))
        .pb(px(12.))
        .rounded(px(10.))
        .border_1()
        .border_color(ui::stage_border())
        .bg(ui::stage_background())
        .child(
            div()
                .flex()
                .gap(px(12.))
                .mb(px(5.))
                .text_size(px(12.5))
                .child(div().flex_1().min_w_0().truncate().child(stage_title(stage)))
                .child(
                    div()
                        .flex_shrink_0()
                        .whitespace_nowrap()
                        .text_color(rgba(ui::SECONDARY))
                        .child(stage.summary()),
                ),
        )
        .child(ui::progress(("stage", index), ratio, fill, 0xffffff1a))
}

/// The label and detail form one text so that the detail is elided first. GPUI elides only text first measured at a
/// known width, so the text sits in a `flex_1` box, whose zero flex basis skips the max-content measurement.
fn stage_title(stage: &Stage) -> StyledText {
    let label = &stage.progress.label;
    match stage.detail() {
        Some(detail) => {
            let detail_start = label.len() + 2;
            let text = format!("{label}  {detail}");
            let end = text.len();
            StyledText::new(text).with_highlights([(
                detail_start..end,
                HighlightStyle {
                    color: Some(rgba(ui::TERTIARY).into()),
                    ..Default::default()
                },
            )])
        }
        None => StyledText::new(label.clone()),
    }
}

fn labeled(label: &'static str, control: impl IntoElement) -> Div {
    div()
        .flex_shrink_0()
        .flex()
        .flex_col()
        .gap(px(5.))
        .child(div().text_size(px(12.)).text_color(rgba(ui::SECONDARY)).child(label))
        .child(control)
}

/// The request as label and value pairs for the details column and the copied log.
fn details(job: &Job) -> Vec<(&'static str, String)> {
    let request = &job.request;
    let mut details = Vec::new();
    if let Some(path) = &request.binary {
        details.push(("Binary", path.display().to_string()));
    }
    if let Some(path) = &request.metadata {
        details.push(("Metadata", path.display().to_string()));
    }
    details.extend([
        ("Output", request.output.display().to_string()),
        ("Game", request.game.clone().unwrap_or_else(|| "Auto-detect".to_owned())),
        ("Targets", job.targets()),
    ]);
    if let Some(path) = &request.startup_metadata {
        details.push(("Startup metadata", path.display().to_string()));
    }
    if let Some(version) = &request.unity_version {
        details.push(("Unity version", version.clone()));
    }
    if let Some(base) = &request.image_base {
        details.push(("Image base", base.clone()));
    }
    details
}

fn status_color(status: Status) -> u32 {
    match status {
        Status::Running => ui::ACCENT_TEXT,
        Status::Succeeded => ui::SUCCESS,
        Status::Failed => ui::DANGER,
        Status::Cancelled => ui::WARNING,
    }
}

fn settings_width(viewport: Size<Pixels>) -> Pixels {
    px(SETTINGS_WIDTH).min((viewport.width - px(32.)).max(px(0.)))
}

/// Explorer's "Copy as path" wraps the path in quotes; relative paths resolve against the GUI's working directory.
fn path(input: &Entity<TextInput>, cx: &App) -> Option<PathBuf> {
    let text = input.read(cx).text().trim().trim_matches('"');
    (!text.is_empty()).then(|| std::path::absolute(text).unwrap_or_else(|_| PathBuf::from(text)))
}

fn text(input: &Entity<TextInput>, cx: &App) -> Option<String> {
    let text = input.read(cx).text().trim();
    (!text.is_empty()).then(|| text.to_owned())
}

/// Unity Windows players keep the metadata in `<Product>_Data/il2cpp_data/Metadata/` beside `GameAssembly.dll`.
fn find_metadata(binary: &Path) -> Option<PathBuf> {
    std::fs::read_dir(binary.parent()?)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path().join("il2cpp_data").join("Metadata").join("global-metadata.dat"))
        .find(|path| path.is_file())
}
