mod drops;
mod features;
mod host;
mod icons;
mod menu;
#[cfg(target_os = "macos")]
mod native_menu;
mod theme;
mod view;
mod window_chrome;

#[cfg(test)]
mod tests;

use iced::{Font, Subscription, Task};
use menu::MenuAction;
use norito_editor_widget::{EditorEvent, Formatting, MarkdownEditor, Mode};
use norito_platform::*;
use norito_platform_native::NativePlatform;
use norito_viewmodel::{Action, Command, EditorViewModel, Effect, Request};
use std::sync::Arc;

fn main() -> iced::Result {
    iced::application(App::boot, App::update, App::view)
        .title(|app: &App| {
            format!(
                "{}{} — norito",
                if app.vm.document.dirty() { "● " } else { "" },
                app.vm.name()
            )
        })
        .theme(|app: &App| theme::theme(app.light))
        .subscription(App::subscription)
        .default_font(Font::with_name("Yu Gothic UI"))
        .window(iced::window::Settings {
            decorations: !cfg!(target_os = "windows"),
            min_size: Some(iced::Size::new(760.0, 520.0)),
            ..Default::default()
        })
        .window_size((1280, 820))
        .centered()
        .exit_on_close_request(false)
        .run()
}

#[derive(Debug, Clone)]
enum Message {
    Feature(features::Event),
    EditBlock(usize, features::Kind),
    Window(window_chrome::Action),
    #[cfg(target_os = "macos")]
    Noop,
    #[cfg(target_os = "macos")]
    NativePoll,
    ContextShortcut(Box<Message>),
    Dropped(std::path::PathBuf),
    DropResolved(norito_platform::Result<FileRef>, u64),
    DropAsset(
        norito_platform::Result<ResourceLink>,
        bool,
        u64,
        u64,
        std::ops::Range<usize>,
    ),
    AnimateTree(std::time::Instant),
    Escape,
    ContextAt(iced::Point),
    MenuToggle(usize),
    MenuHover(usize),
    ModeMenu(bool),
    MenuClose,
    Context(bool),
    EditMenu(EditAction),
    Paste(Option<String>, u64, u64),
    Composing(bool),
    Modifiers(iced::keyboard::Modifiers),
    Command(Command),
    Editor(EditorEvent),
    Mode(Mode),
    ToggleMode,
    Theme,
    Format(Formatting),
    Menu(MenuAction),
    Capture,
    Captured(iced::window::Screenshot),
}
#[derive(Debug, Clone, Copy)]
enum EditAction {
    Cut,
    Copy,
    Paste,
    SelectAll,
}
struct App {
    tree_motion: std::collections::HashMap<DirectoryHandle, (f32, f32, std::time::Instant)>,
    animation_time: std::time::Instant,
    open_menu: Option<usize>,
    mode_menu: bool,
    features: features::Features,
    resources: Arc<dyn DocumentResources>,
    resolving_drop: bool,
    pending_drop_result: Option<Box<Message>>,
    drop_queue: std::collections::VecDeque<(std::path::PathBuf, u64)>,
    native: Arc<NativePlatform>,
    pointer: iced::Point,
    context: Option<iced::Point>,
    composing: bool,
    modifiers: iced::keyboard::Modifiers,
    vm: EditorViewModel,
    editor: MarkdownEditor,
    files: Arc<dyn FileStore>,
    directories: Arc<dyn DirectoryStore>,
    dialogs: Arc<dyn FileDialog>,
    mode: Mode,
    light: bool,
    sidebar: bool,
    screenshot: Option<String>,
}
impl App {
    fn boot() -> (Self, Task<Message>) {
        let native = Arc::new(NativePlatform::default());
        let mut vm = EditorViewModel::default();
        let args: Vec<String> = std::env::args().collect();
        // CLI input also supports repeatable, read-only UI smoke tests.
        if let Some(path) = args.windows(2).find(|a| a[0] == "--folder").map(|a| &a[1]) {
            match native.directory(path.into()).and_then(|root| {
                native
                    .list_children(root.handle)
                    .map(|entries| (root, entries))
            }) {
                Ok((root, entries)) => {
                    vm.children.insert(root.handle, entries);
                    vm.expanded.insert(root.handle);
                    vm.root = Some(root);
                }
                Err(e) => {
                    vm.status = e.message;
                    vm.error = true;
                }
            }
        }
        if let Some(path) = args.windows(2).find(|a| a[0] == "--file").map(|a| &a[1]) {
            match native
                .file(path.into())
                .and_then(|file| native.read(file.handle).map(|read| (file, read)))
            {
                Ok((file, read)) => match norito_model::Document::from_bytes(&read.bytes) {
                    Ok(doc) => {
                        vm.document = doc;
                        vm.file = Some(SaveTarget {
                            file,
                            expected: Some(read.version),
                        });
                    }
                    Err(e) => {
                        vm.status = e;
                        vm.error = true;
                    }
                },
                Err(e) => {
                    vm.status = e.message;
                    vm.error = true;
                }
            }
        }
        let editor = MarkdownEditor::new(&vm.document);
        let screenshot = args
            .windows(2)
            .find(|a| a[0] == "--screenshot")
            .map(|a| a[1].clone());
        let mut app = Self {
            open_menu: None,
            mode_menu: false,
            features: features::Features::default(),
            resources: native.clone(),
            resolving_drop: false,
            pending_drop_result: None,
            tree_motion: Default::default(),
            animation_time: std::time::Instant::now(),
            drop_queue: Default::default(),
            native: native.clone(),
            pointer: iced::Point::new(24.0, 24.0),
            context: None,
            composing: false,
            modifiers: iced::keyboard::Modifiers::default(),
            vm,
            editor,
            files: native.clone(),
            directories: native.clone(),
            dialogs: native,
            mode: Mode::Rich,
            light: args.iter().any(|a| a == "--light"),
            sidebar: true,
            screenshot,
        };
        app.features.outline = args.iter().any(|arg| arg == "--outline");
        let task = if app.screenshot.is_some() {
            Task::perform(
                async { tokio::time::sleep(std::time::Duration::from_secs(3)).await },
                |_| Message::Capture,
            )
        } else {
            Task::none()
        };
        let task = if app.screenshot.is_some() && args.iter().any(|a| a == "--new-directory") {
            Task::batch([task, app.dispatch(Command::NewDirectory, false)])
        } else if app.screenshot.is_some() && args.iter().any(|a| a == "--new-file") {
            Task::batch([task, app.dispatch(Command::Navigate(Action::New), false)])
        } else {
            task
        };
        let task = Task::batch([task, app.refresh_features(), window_chrome::initialize()]);
        if app.screenshot.is_some()
            && args.iter().any(|a| a == "--table-edit")
            && let Some(table) = app.features.structure.tables.first()
        {
            app.editor
                .table_action(norito_editor_widget::TableAction::Focus(
                    table.range.start,
                    0,
                    0,
                ));
        }
        if app.screenshot.is_some() && args.iter().any(|a| a == "--table-picker") {
            app.editor.open_table();
        }
        (app, task)
    }
    fn effects(&self, effects: Vec<Effect>) -> Task<Message> {
        Task::batch(effects.into_iter().map(|effect| {
            if matches!(effect.request, Request::Exit) {
                return iced::exit();
            }
            let files = self.files.clone();
            let directories = self.directories.clone();
            let dialogs = self.dialogs.clone();
            Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || {
                        host::execute(
                            files.as_ref(),
                            directories.as_ref(),
                            dialogs.as_ref(),
                            effect.request,
                        )
                    })
                    .await
                    .unwrap_or_else(|e| {
                        Err(PlatformError {
                            kind: ErrorKind::Io,
                            message: e.to_string(),
                        })
                    })
                },
                move |result| Message::Command(Command::Completed(effect.id, result)),
            )
        }))
    }
    fn dispatch(&mut self, command: Command, from_input: bool) -> Task<Message> {
        if let Command::ToggleDirectory(dir) = &command {
            let now = std::time::Instant::now();
            let from = self.tree_angle(dir.handle);
            let to = if self.vm.expanded.contains(&dir.handle) {
                0.0
            } else {
                90.0
            };
            self.animation_time = now;
            self.tree_motion.insert(dir.handle, (from, to, now));
        }
        if matches!(command, Command::Undo | Command::Redo) {
            self.editor.finish_editing();
        }
        if matches!(command, Command::Cancel | Command::Navigate(_)) {
            self.features.pending_anchor = None;
        }
        let creating = self.vm.creation.is_some();
        let revision = self.vm.document.revision;
        let epoch = self.vm.document_epoch;
        let previous_file = self.vm.file.as_ref().map(|target| target.file.handle);
        let effects = self.vm.dispatch(command);
        let file_changed = previous_file != self.vm.file.as_ref().map(|target| target.file.handle);
        if file_changed {
            self.features.images.clear();
        }
        let changed = revision != self.vm.document.revision || epoch != self.vm.document_epoch;
        if epoch != self.vm.document_epoch {
            self.editor.finish_editing();
        }
        self.editor
            .synchronize(&self.vm.document, changed && !from_input);
        let focus = if !creating && self.vm.creation.is_some() {
            self.sidebar = true;
            iced::widget::operation::focus("new-file")
        } else if (creating && self.vm.creation.is_none()) || epoch != self.vm.document_epoch {
            iced::widget::operation::focus("document")
        } else {
            Task::none()
        };
        let features = if changed || file_changed {
            self.refresh_features()
        } else {
            Task::none()
        };
        let dropped = self.resume_drops();
        Task::batch([self.effects(effects), focus, features, dropped])
    }
    fn update(&mut self, message: Message) -> Task<Message> {
        if self.composing
            && matches!(
                message,
                Message::ToggleMode
                    | Message::Dropped(_)
                    | Message::EditMenu(_)
                    | Message::Context(true)
                    | Message::Mode(_)
                    | Message::Format(_)
                    | Message::Menu(_)
                    | Message::Command(
                        Command::Undo
                            | Command::Redo
                            | Command::CancelCreate
                            | Command::NewDirectory
                            | Command::Navigate(_)
                    )
            )
        {
            return Task::none();
        }
        match message {
            Message::AnimateTree(now) => {
                self.animation_time = now;
                self.tree_motion
                    .retain(|_, (_, _, start)| now.duration_since(*start).as_millis() < 150);
                Task::none()
            }
            Message::MenuToggle(index) => {
                self.mode_menu = false;
                if !self.composing {
                    self.open_menu = if self.open_menu == Some(index) {
                        None
                    } else {
                        Some(index)
                    };
                    self.context = None;
                }
                Task::none()
            }
            Message::MenuHover(index) => {
                if self.open_menu != Some(index) {
                    self.mode_menu = false;
                }
                if self.open_menu.is_some() {
                    self.open_menu = Some(index);
                }
                Task::none()
            }
            Message::MenuClose => {
                self.open_menu = None;
                self.mode_menu = false;
                Task::none()
            }
            Message::ModeMenu(open) => {
                self.mode_menu = open;
                Task::none()
            }
            #[cfg(target_os = "macos")]
            Message::Noop => Task::none(),
            Message::Window(action) => window_chrome::perform(action),
            Message::Feature(event) => self.feature(event),
            Message::EditBlock(offset, kind) => {
                self.editor.jump(offset);
                self.feature(features::Event::Open(kind))
            }
            #[cfg(target_os = "macos")]
            Message::NativePoll => {
                let messages = native_menu::poll();
                Task::batch(messages.into_iter().map(|message| self.update(message)))
            }
            Message::ContextShortcut(message) => {
                if self.context.is_some()
                    || (self.editor.table_active()
                        && matches!(*message, Message::Command(Command::Undo | Command::Redo)))
                {
                    self.update(*message)
                } else {
                    Task::none()
                }
            }
            Message::Escape => {
                if self.open_menu.take().is_some() {
                    return Task::none();
                }
                if self.features.search.is_some() && !self.composing {
                    return self.feature(features::Event::FindClose);
                }
                if self.context.take().is_some() {
                    return iced::widget::operation::focus("document");
                }
                if !self.composing {
                    self.editor.finish_editing();
                    return Task::batch([
                        self.dispatch(Command::CancelCreate, false),
                        iced::widget::operation::focus("document"),
                    ]);
                }
                Task::none()
            }
            Message::ContextAt(point) => {
                self.pointer = point;
                self.update(Message::Context(true))
            }
            Message::Context(open) => {
                self.context = (open && self.vm.can_edit()).then_some(self.pointer);
                Task::none()
            }
            Message::Dropped(path) => {
                let extension = path
                    .extension()
                    .and_then(|s| s.to_str())
                    .unwrap_or_default()
                    .to_ascii_lowercase();
                if !matches!(extension.as_str(), "md" | "markdown") {
                    return self.drop_attachment(path);
                }
                if self.resolving_drop || self.vm.busy() || self.vm.pending.is_some() {
                    return Task::none();
                }
                self.context = None;
                self.resolving_drop = true;
                let native = self.native.clone();
                let epoch = self.vm.document_epoch;
                Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || native.dropped_file(path))
                            .await
                            .unwrap_or_else(|error| {
                                Err(PlatformError {
                                    kind: ErrorKind::Io,
                                    message: error.to_string(),
                                })
                            })
                    },
                    move |result| Message::DropResolved(result, epoch),
                )
            }
            Message::DropResolved(result, epoch) => {
                self.resolving_drop = false;
                if self.composing
                    || self.vm.busy()
                    || self.vm.pending.is_some()
                    || epoch != self.vm.document_epoch
                {
                    return Task::none();
                }
                match result {
                    Ok(file) => self.dispatch(Command::Navigate(Action::OpenFile(file)), false),
                    Err(error) => {
                        self.vm.error = true;
                        self.vm.status = error.message;
                        Task::none()
                    }
                }
            }
            Message::DropAsset(result, image, epoch, revision, selection) => {
                if self.composing {
                    self.pending_drop_result = Some(Box::new(Message::DropAsset(
                        result, image, epoch, revision, selection,
                    )));
                    return Task::none();
                }
                let selection = if revision == self.vm.document.revision {
                    selection
                } else {
                    self.editor.source_selection()
                };
                let revision = self.vm.document.revision;
                // Keep the queue locked until the current insertion has completed.
                let outline = self.features.outline;
                let task = self.feature(features::Event::ResourceReady(
                    result.map(Some),
                    image,
                    epoch,
                    revision,
                    selection,
                ));
                self.features.outline = outline;
                self.resolving_drop = false;
                let next = self.resume_drops();
                Task::batch([task, next])
            }
            Message::EditMenu(action) => {
                self.context = None;
                if !self.vm.can_edit() || self.editor.table_active() {
                    return Task::none();
                }
                use iced::widget::text_editor::{Action as A, Edit};
                let task = match action {
                    EditAction::Copy | EditAction::Cut => {
                        let selected = self.editor.selected_text();
                        if selected.is_empty() {
                            return Task::none();
                        }
                        let copy = iced::clipboard::write(selected);
                        if matches!(action, EditAction::Cut) {
                            Task::batch([
                                copy,
                                self.update(Message::Editor(EditorEvent::Action(A::Edit(
                                    Edit::Paste(Arc::new(String::new())),
                                )))),
                            ])
                        } else {
                            copy
                        }
                    }
                    EditAction::Paste => {
                        let epoch = self.vm.document_epoch;
                        let revision = self.vm.document.revision;
                        iced::clipboard::read()
                            .map(move |text| Message::Paste(text, epoch, revision))
                    }
                    EditAction::SelectAll => {
                        self.update(Message::Editor(EditorEvent::Action(A::SelectAll)))
                    }
                };
                Task::batch([task, iced::widget::operation::focus("document")])
            }
            Message::Paste(text, epoch, revision) => {
                if self.composing
                    || epoch != self.vm.document_epoch
                    || revision != self.vm.document.revision
                {
                    return Task::none();
                }
                text.filter(|text| !text.is_empty())
                    .map_or_else(Task::none, |text| {
                        self.update(Message::Editor(EditorEvent::Action(
                            iced::widget::text_editor::Action::Edit(
                                iced::widget::text_editor::Edit::Paste(Arc::new(text)),
                            ),
                        )))
                    })
            }
            Message::Modifiers(modifiers) => {
                self.modifiers = modifiers;
                Task::none()
            }
            Message::Composing(active) => {
                self.composing = active;
                if !active && let Some(result) = self.pending_drop_result.take() {
                    return Task::done(*result);
                }
                Task::none()
            }
            Message::Command(command) => {
                let menu = self.context.take().is_some();
                let task = self.dispatch(command, false);
                if menu {
                    Task::batch([task, iced::widget::operation::focus("document")])
                } else {
                    task
                }
            }
            Message::Editor(EditorEvent::Format(format)) => self.update(Message::Format(format)),
            Message::Editor(EditorEvent::Undo) => self.update(Message::Command(Command::Undo)),
            Message::Editor(EditorEvent::Redo) => self.update(Message::Command(Command::Redo)),
            Message::Editor(EditorEvent::ContextMenu) => self.update(Message::Context(true)),
            Message::Editor(EditorEvent::EditCode(offset)) => {
                self.editor.edit_code(offset);
                iced::widget::operation::focus("document")
            }
            Message::Editor(EditorEvent::InsertTable) => {
                self.feature(features::Event::Open(features::Kind::Table))
            }
            Message::Editor(EditorEvent::EditTable(offset)) => self.update(Message::Editor(
                EditorEvent::Table(norito_editor_widget::TableAction::Focus(offset, 0, 0)),
            )),
            Message::Editor(EditorEvent::Table(action)) => {
                use norito_editor_widget::TableAction as T;
                if !self.vm.can_edit()
                    || (self.composing && !matches!(action, T::Cell(..) | T::Hover(..)))
                {
                    return Task::none();
                }
                self.context = None;
                let focus = !matches!(action, T::Cell(..) | T::Hover(..));
                let task = if let Some(source) = self.editor.table_action(action) {
                    self.dispatch(Command::Edit(source), false)
                } else {
                    Task::none()
                };
                let focus = if focus {
                    self.editor
                        .table_focus_id()
                        .map_or_else(Task::none, iced::widget::operation::focus)
                } else {
                    Task::none()
                };
                Task::batch([task, focus])
            }
            Message::Editor(EditorEvent::FollowLink) => {
                let caret = self.editor.source_selection().start;
                if let Some(link) = self.features.structure.links.iter().find(|link| {
                    !link.image && link.range.start <= caret && caret <= link.range.end
                }) {
                    return self.feature(features::Event::Follow(link.destination.clone()));
                }
                Task::none()
            }
            Message::Editor(EditorEvent::Action(action)) => {
                self.context = None;
                if !self.vm.can_edit() {
                    return Task::none();
                }
                if let iced::widget::text_editor::Action::Click(point) = &action
                    && let Some(target) = self.editor.click_target(*point)
                {
                    use norito_editor_widget::ClickTarget;
                    let source = match &target {
                        ClickTarget::Link { source, .. } | ClickTarget::Task { source } => *source,
                    };
                    if self.modifiers.control() || self.modifiers.command() {
                        self.editor.edit_code(source);
                        return Task::none();
                    }
                    return match target {
                        ClickTarget::Link { destination, .. } => {
                            self.feature(features::Event::Follow(destination))
                        }
                        ClickTarget::Task { source } => {
                            if let Some(updated) = self.editor.toggle_task_at(source) {
                                let task = self.dispatch(Command::Edit(updated), true);
                                self.editor.finish_editing();
                                task
                            } else {
                                Task::none()
                            }
                        }
                    };
                }
                if self.vm.can_edit()
                    && let Some(source) = self.editor.perform(action)
                {
                    return self.dispatch(Command::Edit(source), true);
                }
                Task::none()
            }
            Message::Mode(mode) => {
                self.context = None;
                if !self.vm.can_edit() {
                    return Task::none();
                }
                self.mode = mode;
                self.editor.set_mode(mode);
                iced::widget::operation::focus("document")
            }
            Message::ToggleMode => self.update(Message::Mode(if self.mode == Mode::Rich {
                Mode::Source
            } else {
                Mode::Rich
            })),
            Message::Format(format) => {
                self.context = None;
                if self.editor.table_active() {
                    return Task::none();
                }
                if self.vm.can_edit()
                    && let Some(source) = self.editor.format(format)
                {
                    return Task::batch([
                        self.dispatch(Command::Edit(source), false),
                        iced::widget::operation::focus("document"),
                    ]);
                }
                Task::none()
            }
            Message::Menu(action) => {
                if action == MenuAction::DisplayMode {
                    self.mode_menu = true;
                    return Task::none();
                }
                self.mode_menu = false;
                self.open_menu = None;
                self.context = None;
                use MenuAction as M;
                let message = match action {
                    M::DisplayMode => unreachable!(),
                    M::Table => Message::Feature(features::Event::Open(features::Kind::Table)),
                    M::CodeBlock => Message::Feature(features::Event::Open(features::Kind::Code)),
                    M::Mermaid => Message::Feature(features::Event::Open(features::Kind::Mermaid)),
                    M::Image => Message::Feature(features::Event::Resource(true)),
                    M::PageLink => Message::Feature(features::Event::Resource(false)),
                    M::Outline => Message::Feature(features::Event::ToggleOutline),
                    M::New => Message::Command(Command::Navigate(Action::New)),
                    M::NewDirectory => Message::Command(Command::NewDirectory),
                    M::Open => Message::Command(Command::Navigate(Action::Open)),
                    M::Folder => Message::Command(Command::OpenFolder),
                    M::Save => Message::Command(Command::Save(false)),
                    M::SaveAs => Message::Command(Command::Save(true)),
                    M::Refresh => Message::Command(Command::Refresh),
                    M::Rich => Message::Mode(Mode::Rich),
                    M::Source => Message::Mode(Mode::Source),
                    M::Theme => Message::Theme,
                    M::Sidebar => {
                        self.sidebar = !self.sidebar;
                        return Task::none();
                    }
                    M::Bold => Message::Format(Formatting::Bold),
                    M::Italic => Message::Format(Formatting::Italic),
                    M::Code => Message::Format(Formatting::Code),
                    M::Paragraph => Message::Format(Formatting::Heading(0)),
                    M::H1 => Message::Format(Formatting::Heading(1)),
                    M::H2 => Message::Format(Formatting::Heading(2)),
                    M::H3 => Message::Format(Formatting::Heading(3)),
                    M::Bullet => Message::Format(Formatting::Bullet),
                    M::Quote => Message::Format(Formatting::Quote),
                    M::Strike => Message::Format(Formatting::Strike),
                    M::Find => Message::Feature(features::Event::Find),
                };
                self.update(message)
            }
            Message::Theme => {
                self.light = !self.light;
                Task::none()
            }
            Message::Capture => iced::window::latest()
                .and_then(iced::window::screenshot)
                .map(Message::Captured),
            Message::Captured(shot) => {
                if let Some(path) = &self.screenshot {
                    let file = std::fs::File::create(path).expect("screenshot output");
                    let mut encoder = png::Encoder::new(
                        std::io::BufWriter::new(file),
                        shot.size.width,
                        shot.size.height,
                    );
                    encoder.set_color(png::ColorType::Rgba);
                    encoder.set_depth(png::BitDepth::Eight);
                    encoder
                        .write_header()
                        .expect("PNG header")
                        .write_image_data(&shot.rgba)
                        .expect("PNG data");
                    println!("Screenshot saved: {path}");
                }
                iced::exit()
            }
        }
    }
    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            if self.tree_motion.is_empty() {
                Subscription::none()
            } else {
                iced::time::every(std::time::Duration::from_millis(16)).map(Message::AnimateTree)
            },
            window_chrome::subscription(),
            iced::window::close_requests()
                .map(|_| Message::Command(Command::Navigate(Action::Exit))),
            iced::event::listen_with(|event, status, _| {
                if let iced::Event::Window(iced::window::Event::FileDropped(path)) = &event {
                    return Some(Message::Dropped(path.clone()));
                }
                if let iced::Event::InputMethod(event) = &event {
                    return match event {
                        iced::advanced::input_method::Event::Preedit(text, _) => {
                            Some(Message::Composing(!text.is_empty()))
                        }
                        iced::advanced::input_method::Event::Closed
                        | iced::advanced::input_method::Event::Commit(_) => {
                            Some(Message::Composing(false))
                        }
                        _ => None,
                    };
                }
                if matches!(event, iced::Event::Window(iced::window::Event::Unfocused)) {
                    return Some(Message::Composing(false));
                }
                if let iced::Event::Keyboard(iced::keyboard::Event::ModifiersChanged(modifiers)) =
                    &event
                {
                    return Some(Message::Modifiers(*modifiers));
                }
                let iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                    key,
                    modifiers,
                    repeat,
                    ..
                }) = event
                else {
                    return None;
                };
                if key == iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape) {
                    return Some(Message::Escape);
                }
                if status == iced::event::Status::Ignored {
                    use iced::keyboard::{Key, key::Named};
                    use norito_editor_widget::TableAction as T;
                    if key == Key::Named(Named::Tab) {
                        return Some(Message::Editor(EditorEvent::Table(T::Move(
                            modifiers.shift(),
                        ))));
                    }
                    if key == Key::Named(Named::Enter) && modifiers.command() {
                        return Some(Message::Editor(EditorEvent::Table(T::Row(1))));
                    }
                }
                if modifiers.command()
                    && let iced::keyboard::Key::Character(key) = key
                {
                    {
                        let message = match key.to_lowercase().as_str() {
                            "x" => Some(Message::EditMenu(EditAction::Cut)),
                            "c" => Some(Message::EditMenu(EditAction::Copy)),
                            "v" => Some(Message::EditMenu(EditAction::Paste)),
                            "a" => Some(Message::EditMenu(EditAction::SelectAll)),
                            "z" => Some(Message::Command(if modifiers.shift() {
                                Command::Redo
                            } else {
                                Command::Undo
                            })),
                            "y" => Some(Message::Command(Command::Redo)),
                            "b" => Some(Message::Format(Formatting::Bold)),
                            "i" => Some(Message::Format(Formatting::Italic)),
                            "e" => Some(Message::Format(Formatting::Code)),
                            _ => None,
                        };
                        if let Some(message) = message {
                            return (status == iced::event::Status::Ignored)
                                .then(|| Message::ContextShortcut(Box::new(message)));
                        }
                    }
                    return match key.to_lowercase().as_str() {
                        "f" | "h" => Some(Message::Feature(features::Event::Find)),
                        "/" if !repeat && !modifiers.alt() && !modifiers.shift() => {
                            Some(Message::ToggleMode)
                        }
                        "s" => Some(Message::Command(Command::Save(modifiers.shift()))),
                        "o" => Some(Message::Command(if modifiers.shift() {
                            Command::OpenFolder
                        } else {
                            Command::Navigate(Action::Open)
                        })),
                        "n" => Some(Message::Command(if modifiers.shift() {
                            Command::NewDirectory
                        } else {
                            Command::Navigate(Action::New)
                        })),
                        _ => None,
                    };
                }
                None
            }),
        ])
    }
}
