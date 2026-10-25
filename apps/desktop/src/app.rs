//! Application state and the update loop. `update` only routes; each message
//! is handled by the module that owns its state.
use crate::{
    cli::Options, drops::Drops, features, features::Features, host, keyboard, menu::MenuAction,
    menu::MenuBar, screenshot, services::Services, view::Chevrons, window_chrome,
};
use iced::{Point, Subscription, Task, keyboard::Modifiers, widget::operation::focus};
use norito_editor_widget::{EditorEvent, Formatting, MarkdownEditor, Mode, TableAction};
use norito_platform::{FileRef, ResourceLink, Result};
use norito_viewmodel::{Action, Command, EditorViewModel};

#[derive(Debug, Clone)]
pub enum Message {
    Feature(features::Event),
    EditBlock(usize, features::Kind),
    Window(window_chrome::Action),
    #[cfg(target_os = "macos")]
    Noop,
    #[cfg(target_os = "macos")]
    NativePoll,
    /// A shortcut the focused widget did not handle; applies only in some states.
    ContextShortcut(Box<Message>),
    Dropped(std::path::PathBuf),
    DropResolved(Result<FileRef>, u64),
    DropAsset(Result<ResourceLink>, features::Placement),
    AnimateTree(std::time::Instant),
    Escape,
    ContextAt(Point),
    MenuToggle(usize),
    MenuHover(usize),
    ModeMenu(bool),
    MenuClose,
    Context(bool),
    EditMenu(EditAction),
    Paste(Option<String>, u64, u64),
    ClipboardWritten(
        norito_platform::Result<()>,
        Option<std::ops::Range<usize>>,
        u64,
        u64,
    ),
    Composing(bool),
    Modifiers(Modifiers),
    Command(Command),
    Editor(EditorEvent),
    Mode(Mode),
    ToggleMode,
    Theme,
    Sidebar,
    Format(Formatting),
    Menu(MenuAction),
    Capture,
    Captured(iced::window::Screenshot),
}
#[derive(Debug, Clone, Copy)]
pub enum EditAction {
    Cut,
    Copy,
    Paste,
    SelectAll,
}

pub struct App {
    pub(crate) vm: EditorViewModel,
    pub(crate) editor: MarkdownEditor,
    pub(crate) features: Features,
    pub(crate) services: Services,
    pub(crate) drops: Drops,
    pub(crate) menu: MenuBar,
    pub(crate) chevrons: Chevrons,
    /// Last right-click position, where the context menu opens.
    pub(crate) pointer: Point,
    pub(crate) context: Option<Point>,
    /// IME composition in progress: document-changing messages wait.
    pub(crate) composing: bool,
    pub(crate) modifiers: Modifiers,
    pub(crate) mode: Mode,
    pub(crate) light: bool,
    pub(crate) sidebar: bool,
    pub(crate) screenshot: Option<String>,
}

impl App {
    pub fn boot() -> (Self, Task<Message>) {
        let args: Vec<String> = std::env::args().collect();
        let options = Options::parse(&args);
        let services = Services::native();
        let mut vm = EditorViewModel::default();
        options.load(&services, &mut vm);
        let mut app = Self {
            editor: MarkdownEditor::new(&vm.session.document),
            vm,
            features: Features::default(),
            services,
            drops: Drops::default(),
            menu: MenuBar::default(),
            chevrons: Chevrons::default(),
            pointer: Point::new(24.0, 24.0),
            context: None,
            composing: false,
            modifiers: Modifiers::default(),
            mode: Mode::Rich,
            light: options.light,
            sidebar: true,
            screenshot: options.screenshot.clone(),
        };
        app.features.outline = options.outline;
        let mut tasks = vec![app.refresh_features(), window_chrome::initialize()];
        if app.screenshot.is_some() {
            tasks.push(screenshot::schedule());
            if options.new_directory {
                tasks.push(app.dispatch(Command::NewDirectory, false));
            } else if options.new_file {
                tasks.push(app.dispatch(Command::Navigate(Action::New), false));
            }
            if options.table_edit
                && let Some(table) = app.features.structure.tables.first()
            {
                app.editor
                    .table_action(TableAction::Focus(table.range.start, 0, 0));
            }
            if options.table_picker {
                app.editor.open_table();
            }
        }
        (app, Task::batch(tasks))
    }
    pub fn title(&self) -> String {
        format!(
            "{}{} — norito",
            if self.vm.session.document.dirty() {
                "● "
            } else {
                ""
            },
            self.vm.session.name()
        )
    }
    /// Run a view-model command and bring the editor, features and focus in line with it.
    /// `from_input` means the editor already shows the edit.
    pub(crate) fn dispatch(&mut self, command: Command, from_input: bool) -> Task<Message> {
        if let Command::ToggleDirectory(dir) = &command {
            let expanded = self.vm.workspace.expanded.contains(&dir.handle);
            self.chevrons.start(dir.handle, expanded);
        }
        if matches!(command, Command::Undo | Command::Redo) {
            self.editor.finish_editing();
        }
        if matches!(command, Command::Cancel | Command::Navigate(_)) {
            self.features.pending_anchor = None;
        }
        let creating = self.vm.creation.is_some();
        let revision = self.vm.session.document.revision;
        let epoch = self.vm.session.epoch;
        let previous_file = self.vm.session.handle();
        let effects = self.vm.dispatch(command);
        let file_changed = previous_file != self.vm.session.handle();
        if file_changed {
            self.features.media.images.clear();
        }
        let switched = epoch != self.vm.session.epoch;
        let changed = revision != self.vm.session.document.revision || switched;
        if switched {
            self.editor.finish_editing();
        }
        self.editor
            .synchronize(&self.vm.session.document, changed && !from_input);
        let focus = if !creating && self.vm.creation.is_some() {
            self.sidebar = true;
            focus("new-file")
        } else if (creating && self.vm.creation.is_none()) || switched {
            focus("document")
        } else {
            Task::none()
        };
        let features = if changed || file_changed {
            self.refresh_features()
        } else {
            Task::none()
        };
        let dropped = self.resume_drops();
        Task::batch([
            host::perform(&self.services, effects),
            focus,
            features,
            dropped,
        ])
    }
    /// Messages that would change the document or its view wait for IME composition.
    fn blocked_by_composition(message: &Message) -> bool {
        matches!(
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
    }
    pub fn update(&mut self, message: Message) -> Task<Message> {
        if self.composing && Self::blocked_by_composition(&message) {
            return Task::none();
        }
        match message {
            Message::AnimateTree(now) => {
                self.chevrons.tick(now);
                Task::none()
            }
            Message::MenuToggle(index) => {
                if self.composing {
                    self.menu.modes = false;
                } else {
                    self.menu.toggle(index);
                    self.context = None;
                }
                Task::none()
            }
            Message::MenuHover(index) => {
                self.menu.hover(index);
                Task::none()
            }
            Message::MenuClose => {
                self.menu.close();
                Task::none()
            }
            Message::ModeMenu(open) => {
                self.menu.modes = open;
                Task::none()
            }
            #[cfg(target_os = "macos")]
            Message::Noop => Task::none(),
            #[cfg(target_os = "macos")]
            Message::NativePoll => {
                let messages = crate::native_menu::poll();
                Task::batch(messages.into_iter().map(|message| self.update(message)))
            }
            Message::Window(action) => window_chrome::perform(action),
            Message::Feature(event) => self.feature(event),
            Message::EditBlock(offset, kind) => {
                self.editor.jump(offset);
                self.feature(features::Event::Open(kind))
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
            Message::Escape => self.escape(),
            Message::ContextAt(point) => {
                self.pointer = point;
                self.update(Message::Context(true))
            }
            Message::Context(open) => {
                self.context = (open && self.vm.can_edit()).then_some(self.pointer);
                Task::none()
            }
            Message::Dropped(path) => self.dropped(path),
            Message::DropResolved(result, epoch) => self.drop_resolved(result, epoch),
            Message::DropAsset(result, placement) => self.drop_imported(result, placement),
            Message::EditMenu(action) => self.edit_menu(action),
            Message::Paste(text, epoch, revision) => self.paste(text, epoch, revision),
            Message::ClipboardWritten(result, cut, epoch, revision) => {
                self.clipboard_written(result, cut, epoch, revision)
            }
            Message::Modifiers(modifiers) => {
                self.modifiers = modifiers;
                Task::none()
            }
            Message::Composing(active) => {
                self.composing = active;
                if !active && let Some(result) = self.drops.deferred.take() {
                    return Task::done(*result);
                }
                Task::none()
            }
            Message::Command(command) => {
                let menu = self.context.take().is_some();
                let task = self.dispatch(command, false);
                if menu {
                    Task::batch([task, focus("document")])
                } else {
                    task
                }
            }
            Message::Editor(event) => self.editor_event(event),
            Message::Mode(mode) => self.set_mode(mode),
            Message::ToggleMode => self.update(Message::Mode(match self.mode {
                Mode::Rich => Mode::Source,
                Mode::Source => Mode::Rich,
            })),
            Message::Format(format) => self.format(format),
            Message::Menu(action) => {
                if action == MenuAction::DisplayMode {
                    self.menu.modes = true;
                    return Task::none();
                }
                self.menu.close();
                self.context = None;
                action
                    .message()
                    .map_or_else(Task::none, |message| self.update(message))
            }
            Message::Theme => {
                self.light = !self.light;
                Task::none()
            }
            Message::Sidebar => {
                self.sidebar = !self.sidebar;
                Task::none()
            }
            Message::Capture => screenshot::capture(),
            Message::Captured(shot) => {
                if let Some(path) = &self.screenshot {
                    screenshot::save(path, &shot);
                }
                iced::exit()
            }
        }
    }
    /// Close the innermost open thing: menu, search, context menu, then revealed syntax.
    fn escape(&mut self) -> Task<Message> {
        if self.menu.open.take().is_some() {
            return Task::none();
        }
        if self.features.search.is_some() && !self.composing {
            return self.feature(features::Event::FindClose);
        }
        if self.context.take().is_some() {
            return focus("document");
        }
        if self.composing {
            return Task::none();
        }
        self.editor.finish_editing();
        Task::batch([
            self.dispatch(Command::CancelCreate, false),
            focus("document"),
        ])
    }
    pub fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            if self.chevrons.animating() {
                iced::time::every(std::time::Duration::from_millis(16)).map(Message::AnimateTree)
            } else {
                Subscription::none()
            },
            window_chrome::subscription(),
            iced::window::close_requests()
                .map(|_| Message::Command(Command::Navigate(Action::Exit))),
            keyboard::listen(),
        ])
    }
}
