use std::borrow::Cow;
use std::cell::RefCell;
use std::io::{self, Read};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
    KeyModifiers,
};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen, SetTitle,
};
use ratatui::backend::{Backend, CrosstermBackend};
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use ratatui::{Frame, Terminal};

use crate::link::Link;
use crate::lock::LockFile;
use crate::project::Project;
use crate::requests::{InsertRequest, RemoteRequest};

use cli_clipboard::{ClipboardContext, ClipboardProvider};

pub fn main(project: Project, listener: TcpListener, lockfile: LockFile) {
    let mut stdout = io::stdout();
    enable_raw_mode().expect("Failed to enable raw mode");
    execute!(
        stdout,
        EnterAlternateScreen,
        EnableMouseCapture,
        SetTitle("Flist")
    )
    .expect("Failed to enter alternate screen");

    let mut terminal =
        Terminal::new(CrosstermBackend::new(stdout)).expect("Failed to create terminal");

    let tick_rate = Duration::from_millis(100);
    let app = App::new(project, lockfile, ClipboardContext::new().ok());
    start_listener_thread(&app, listener);
    let result = run_app(&mut terminal, app, tick_rate);

    disable_raw_mode().expect("Failed to disable raw mode");
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )
    .expect("Failed to leave alternate screen");
    terminal.show_cursor().expect("Failed to show cursor");

    result.expect("Failed to run app");
}

type PendingMessages = Arc<Mutex<Vec<ListenerMessages>>>;

fn handle_stream(mut stream: TcpStream, pending_messages: PendingMessages) {
    let mut buffer = String::new();
    stream.read_to_string(&mut buffer).unwrap();
    if buffer.is_empty() {
        return;
    }
    let Ok(request) = serde_json::from_str::<RemoteRequest>(&buffer) else {return;};
    pending_messages.lock().unwrap().push(request.into());
}

fn start_listener_thread(app: &App, listener: TcpListener) {
    let pending_messages = app.pending_messages.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let pending_messages = pending_messages.clone();
            std::thread::spawn(move || handle_stream(stream, pending_messages));
        }
    });
}

struct App {
    project: Project,
    _lockfile: LockFile,

    pending_messages: PendingMessages,

    select_state: SelectState,
    clipboard: Option<RefCell<ClipboardContext>>,
}

impl App {
    fn new(project: Project, lockfile: LockFile, clipboard: Option<ClipboardContext>) -> Self {
        Self {
            project,
            _lockfile: lockfile,
            pending_messages: Arc::new(Mutex::new(Vec::new())),
            select_state: SelectState::Entry(0),
            clipboard: clipboard.map(RefCell::new),
        }
    }

    fn apply_messages(&mut self) {
        let messages = self
            .pending_messages
            .lock()
            .unwrap()
            .drain(..)
            .collect::<Vec<_>>();
        let mut should_save = false;
        for message in messages {
            should_save |= message.apply(self);
        }
        if should_save {
            self.project.save();
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum SelectState {
    Entry(usize), // the usize will always be the index of the entry in the project, except if the project is empty, in which case it will be 0
    Archive(usize),
    Drag {
        dragged_entry_idx: usize,
        new_position: usize,
    },
}

impl SelectState {
    fn on_event(
        &self,
        event: Event,
        project: &mut Project,
        clipboard: &Option<RefCell<ClipboardContext>>,
    ) -> OnEvent {
        if let Event::Key(KeyEvent {
            code: KeyCode::Char('q'),
            ..
        }) = event
        {
            return OnEvent::exit();
        }
        match self {
            Self::Entry(selected_idx) => {
                let selected_idx = *selected_idx;
                match event {
                    Event::Key(KeyEvent {
                        code: KeyCode::Up,
                        kind: KeyEventKind::Press,
                        ..
                    }) if !project.entries.is_empty() && selected_idx > 0 => {
                        OnEvent::without_saving(Self::Entry(selected_idx - 1))
                    }
                    Event::Key(KeyEvent {
                        code: KeyCode::Down,
                        kind: KeyEventKind::Press,
                        ..
                    }) if !project.entries.is_empty()
                        && selected_idx < project.entries.len() - 1 =>
                    {
                        OnEvent::without_saving(Self::Entry(selected_idx + 1))
                    }
                    Event::Key(KeyEvent {
                        code: KeyCode::Delete,
                        kind: KeyEventKind::Press,
                        ..
                    }) if !project.entries.is_empty() => {
                        project.archive_entry(selected_idx);
                        let new_idx = if !project.entries.is_empty()
                            && selected_idx == project.entries.len()
                        {
                            selected_idx - 1
                        } else {
                            selected_idx
                        };
                        OnEvent::with_saving(Self::Entry(new_idx))
                    }
                    Event::Key(KeyEvent {
                        code: KeyCode::Char('a'),
                        kind: KeyEventKind::Press,
                        ..
                    }) if !project.archive.is_empty() => OnEvent::without_saving(Self::Archive(0)),
                    Event::Key(KeyEvent {
                        code: KeyCode::Char('d'),
                        kind: KeyEventKind::Press,
                        ..
                    }) if !project.entries.is_empty() => OnEvent::without_saving(Self::Drag {
                        dragged_entry_idx: selected_idx,
                        new_position: selected_idx,
                    }),
                    Event::Key(KeyEvent {
                        code: KeyCode::Home,
                        kind: KeyEventKind::Press,
                        ..
                    }) => OnEvent::without_saving(Self::Entry(0)),
                    Event::Key(KeyEvent {
                        code: KeyCode::End,
                        kind: KeyEventKind::Press,
                        ..
                    }) if !project.entries.is_empty() => {
                        OnEvent::without_saving(Self::Entry(project.entries.len() - 1))
                    }
                    Event::Key(KeyEvent {
                        code: KeyCode::Enter,
                        kind: KeyEventKind::Press,
                        modifiers,
                        ..
                    }) if !project.entries.is_empty() => {
                        let entry = &project.entries[selected_idx];
                        if modifiers.contains(KeyModifiers::CONTROL) {
                            if let Ok(Some(pref)) = entry
                                .link
                                .preferred_file(project.config.preferred_suffixes.iter())
                            {
                                pref.open();
                            } else {
                                entry.link.explore()
                            }
                        } else {
                            entry.link.explore()
                        };
                        OnEvent::ignore()
                    }
                    Event::Key(KeyEvent {
                        code: KeyCode::Char('v'),
                        modifiers: KeyModifiers::CONTROL,
                        kind: KeyEventKind::Press,
                        ..
                    }) => {
                        if let Some(clipboard) = &clipboard {
                            if let Ok(contents) = clipboard.borrow_mut().get_contents() {
                                let link = Link::from(contents.as_str());
                                let name = link.infer_name();
                                let request = InsertRequest {
                                    name,
                                    link,
                                    metadata: Vec::new(),
                                };
                                let new_idx = if project.entries.is_empty() {
                                    0
                                } else {
                                    selected_idx + 1
                                };
                                project.insert_entry_at(request.into(), new_idx);
                                OnEvent::with_saving(Self::Entry(new_idx))
                            } else {
                                OnEvent::ignore()
                            }
                        } else {
                            OnEvent::ignore()
                        }
                    }
                    _ => OnEvent::ignore(),
                }
            }
            Self::Archive(selected_idx) => {
                let selected_idx = *selected_idx;
                match event {
                    Event::Key(KeyEvent {
                        code: KeyCode::Up,
                        kind: KeyEventKind::Press,
                        ..
                    }) if selected_idx > 0 => {
                        OnEvent::without_saving(Self::Archive(selected_idx - 1))
                    }
                    Event::Key(KeyEvent {
                        code: KeyCode::Down,
                        kind: KeyEventKind::Press,
                        ..
                    }) if selected_idx < project.archive.len() - 1 => {
                        OnEvent::without_saving(Self::Archive(selected_idx + 1))
                    }
                    Event::Key(KeyEvent {
                        code: KeyCode::Delete,
                        kind: KeyEventKind::Press,
                        ..
                    }) => {
                        project.remove_from_archive(selected_idx);
                        OnEvent::with_saving(if project.archive.is_empty() {
                            Self::Entry(0)
                        } else if selected_idx == project.archive.len() {
                            Self::Archive(selected_idx - 1)
                        } else {
                            Self::Archive(selected_idx)
                        })
                    }
                    Event::Key(KeyEvent {
                        code: KeyCode::Char('a'),
                        kind: KeyEventKind::Press,
                        ..
                    }) => OnEvent::without_saving(Self::Entry(0)),
                    Event::Key(KeyEvent {
                        code: KeyCode::Char('r'),
                        kind: KeyEventKind::Press,
                        ..
                    }) => {
                        project.restore_from_archive(selected_idx);
                        OnEvent::with_saving(Self::Entry(0))
                    }
                    Event::Key(KeyEvent {
                        code: KeyCode::Home,
                        kind: KeyEventKind::Press,
                        ..
                    }) => OnEvent::without_saving(Self::Archive(0)),
                    Event::Key(KeyEvent {
                        code: KeyCode::End,
                        kind: KeyEventKind::Press,
                        ..
                    }) => OnEvent::without_saving(Self::Archive(project.entries.len() - 1)),
                    Event::Key(KeyEvent {
                        code: KeyCode::Enter,
                        kind: KeyEventKind::Press,
                        modifiers,
                        ..
                    }) if !project.entries.is_empty() => {
                        let entry = &project.archive[selected_idx];
                        if modifiers.contains(KeyModifiers::CONTROL) {
                            if let Ok(Some(pref)) = entry
                                .link
                                .preferred_file(project.config.preferred_suffixes.iter())
                            {
                                pref.open();
                            } else {
                                entry.link.explore()
                            }
                        } else {
                            entry.link.explore()
                        };
                        OnEvent::ignore()
                    }
                    _ => OnEvent::ignore(),
                }
            }
            Self::Drag {
                dragged_entry_idx,
                new_position,
            } => {
                let dragged_entry_idx = *dragged_entry_idx;
                let new_position = *new_position;
                match event {
                    Event::Key(KeyEvent {
                        code: KeyCode::Up,
                        kind: KeyEventKind::Press,
                        ..
                    }) if new_position > 0 => OnEvent::without_saving(Self::Drag {
                        dragged_entry_idx,
                        new_position: new_position - 1,
                    }),
                    Event::Key(KeyEvent {
                        code: KeyCode::Down,
                        kind: KeyEventKind::Press,
                        ..
                    }) if new_position < project.entries.len() - 1 => {
                        OnEvent::without_saving(Self::Drag {
                            dragged_entry_idx,
                            new_position: new_position + 1,
                        })
                    }
                    Event::Key(KeyEvent {
                        code: KeyCode::Home,
                        kind: KeyEventKind::Press,
                        ..
                    }) => OnEvent::without_saving(Self::Drag {
                        dragged_entry_idx,
                        new_position: 0,
                    }),
                    Event::Key(KeyEvent {
                        code: KeyCode::End,
                        kind: KeyEventKind::Press,
                        ..
                    }) => OnEvent::without_saving(Self::Drag {
                        dragged_entry_idx,
                        new_position: project.entries.len() - 1,
                    }),
                    Event::Key(KeyEvent {
                        code: KeyCode::Enter,
                        kind: KeyEventKind::Press,
                        ..
                    }) => {
                        project.move_entry(dragged_entry_idx, new_position);
                        OnEvent::with_saving(Self::Entry(new_position))
                    }
                    Event::Key(KeyEvent {
                        code: KeyCode::Esc,
                        kind: KeyEventKind::Press,
                        ..
                    }) => OnEvent::without_saving(Self::Entry(dragged_entry_idx)),
                    _ => OnEvent::ignore(),
                }
            }
        }
    }

    fn get_options(&self, app: &App) -> Vec<KeyOption> {
        let mut ret = Vec::new();
        match self {
            SelectState::Entry(selected_idx) => {
                let selected_idx = *selected_idx;
                if !app.project.entries.is_empty() {
                    ret.push(KeyOption::new("<Enter>", "open entry"));
                    let entry = &app.project.entries[selected_idx];
                    if let Ok(Some(pref)) = entry
                        .link
                        .preferred_file(app.project.config.preferred_suffixes.iter())
                    {
                        let desc = match &pref.extension {
                            Some(ext) => format!("open .{} file", ext.to_uppercase()).into(),
                            None => Cow::Borrowed("open preferred file"),
                        };
                        ret.push(KeyOption::new("<Ctrl+Enter>", desc));
                    }
                    if selected_idx > 0 {
                        ret.push(KeyOption::new("<Up>", "select above entry"));
                    }
                    if selected_idx < app.project.entries.len() - 1 {
                        ret.push(KeyOption::new("<Down>", "select below entry"));
                    }
                    ret.push(KeyOption::new("<Home>", "select first entry"));
                    ret.push(KeyOption::new("<End>", "select last entry"));
                    ret.push(KeyOption::new("<Delete>", "archive entry"));
                    ret.push(KeyOption::new("d", "drag entry"));
                }
                if !app.project.archive.is_empty() {
                    ret.push(KeyOption::new("a", "go to archive"));
                }
                if let Some(clipboard) = &app.clipboard {
                    if clipboard.borrow_mut().get_contents().is_ok() {
                        ret.push(KeyOption::new("^v", "paste clipboard"));
                    }
                }
            }
            SelectState::Archive(selected_idx) => {
                let selected_idx = *selected_idx;
                ret.push(KeyOption::new("<Enter>", "open entry"));
                let entry = &app.project.archive[selected_idx];
                if let Ok(Some(pref)) = entry
                    .link
                    .preferred_file(app.project.config.preferred_suffixes.iter())
                {
                    let desc = match &pref.extension {
                        Some(ext) => format!("open .{} file", ext.to_uppercase()).into(),
                        None => Cow::Borrowed("open preferred file"),
                    };
                    ret.push(KeyOption::new("<Ctrl+Enter>", desc));
                }
                if selected_idx > 0 {
                    ret.push(KeyOption::new("<Up>", "select above entry"));
                }
                if selected_idx < app.project.archive.len() - 1 {
                    ret.push(KeyOption::new("<Down>", "select below entry"));
                }
                ret.push(KeyOption::new("<Home>", "select first entry"));
                ret.push(KeyOption::new("<End>", "select last entry"));
                ret.push(KeyOption::new("<Delete>", "delete entry forever"));
                ret.push(KeyOption::new("r", "restore entry"));
                ret.push(KeyOption::new("a", "return to main entries"));
            }
            SelectState::Drag { new_position, .. } => {
                let new_position = *new_position;
                ret.push(KeyOption::new("<Enter>", "select new location"));
                if new_position > 0 {
                    ret.push(KeyOption::new("<Up>", "shift one up"));
                }
                if new_position < app.project.entries.len() - 1 {
                    ret.push(KeyOption::new("<Down>", "shift one down"));
                }
                ret.push(KeyOption::new("<Home>", "shift to top"));
                ret.push(KeyOption::new("<End>", "shift to bottom"));
                ret.push(KeyOption::new("<Esc>", "cancel drag"));
            }
        }
        ret.push(KeyOption::new("q", "quit"));
        ret
    }
}

struct KeyOption {
    key: &'static str,
    description: Cow<'static, str>,
}

impl KeyOption {
    fn new(key: &'static str, description: impl Into<Cow<'static, str>>) -> Self {
        Self {
            key,
            description: description.into(),
        }
    }

    fn to_line(&self) -> Line<'static> {
        Line::from(vec![
            Span::styled(self.key, Style::default().add_modifier(Modifier::BOLD)),
            Span::raw("- "),
            Span::raw(self.description.clone()),
        ])
    }
}

struct OnEvent {
    next_state: Option<NextState>,
    save: bool,
}

enum NextState {
    Exit,
    State(SelectState),
}

impl OnEvent {
    fn exit() -> Self {
        Self {
            next_state: Some(NextState::Exit),
            save: false,
        }
    }

    fn without_saving(state: SelectState) -> Self {
        Self {
            next_state: Some(NextState::State(state)),
            save: false,
        }
    }

    fn with_saving(state: SelectState) -> Self {
        Self {
            next_state: Some(NextState::State(state)),
            save: true,
        }
    }

    fn ignore() -> Self {
        Self {
            next_state: None,
            save: false,
        }
    }
}

enum ListenerMessages {
    Insert(InsertRequest),
}

impl ListenerMessages {
    fn apply(self, app: &mut App) -> bool {
        // returns swhether a save is needed
        match self {
            ListenerMessages::Insert(request) => {
                app.project.insert_entry(request.into());
                true
            }
        }
    }
}

impl From<RemoteRequest> for ListenerMessages {
    fn from(request: RemoteRequest) -> Self {
        match request {
            RemoteRequest::Insert(request) => Self::Insert(request),
        }
    }
}

fn run_app<B: Backend>(
    terminal: &mut Terminal<B>,
    mut app: App,
    tick_rate: Duration,
) -> io::Result<()> {
    loop {
        app.apply_messages();
        terminal.draw(|f| ui(f, &mut app))?;

        let timeout = tick_rate;
        if crossterm::event::poll(timeout)? {
            let ev = event::read()?;
            let on_event = app
                .select_state
                .on_event(ev, &mut app.project, &app.clipboard);
            if on_event.save {
                app.project.save();
            }

            match on_event.next_state {
                None => {}
                Some(NextState::Exit) => {
                    break Ok(());
                }
                Some(NextState::State(new_state)) => {
                    app.select_state = new_state;
                }
            }
        }
    }
}

fn ui<B: Backend>(f: &mut Frame<B>, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)].as_ref())
        .split(f.size());

    let bottom_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)].as_ref())
        .split(chunks[1]);

    let (entrylist, mut list_state, block_title) = match app.select_state {
        SelectState::Entry(selected_idx) => (
            Cow::Borrowed(&app.project.entries),
            ListState::default().with_selected(Some(selected_idx)),
            "Entries",
        ),
        SelectState::Archive(selected_idx) => (
            Cow::Borrowed(&app.project.archive),
            ListState::default().with_selected(Some(selected_idx)),
            "Archive",
        ),
        SelectState::Drag {
            dragged_entry_idx,
            new_position,
        } => {
            let mut entries = app.project.entries.clone();
            let dragged_entry = entries.remove(dragged_entry_idx);
            entries.insert(new_position, dragged_entry);
            (
                Cow::Owned(entries),
                ListState::default().with_selected(Some(new_position)),
                "Entries",
            )
        }
    };

    let highlight_modifier = if let SelectState::Drag { .. } = app.select_state {
        Modifier::REVERSED
    } else {
        Modifier::BOLD
    };

    let list = List::new(
        entrylist
            .iter()
            .map(|entry| ListItem::new(entry.name.clone()))
            .collect::<Vec<_>>(),
    )
    .block(Block::default().borders(Borders::ALL).title(block_title))
    .highlight_style(Style::default().add_modifier(highlight_modifier))
    .highlight_symbol(">>");

    f.render_stateful_widget(list, chunks[0], &mut list_state);

    let selected_entry = match app.select_state {
        SelectState::Entry(0) if app.project.entries.is_empty() => None,
        SelectState::Entry(selected_idx) => Some(&app.project.entries[selected_idx]),
        SelectState::Archive(selected_idx) => Some(&app.project.archive[selected_idx]),
        SelectState::Drag {
            dragged_entry_idx, ..
        } => Some(&app.project.entries[dragged_entry_idx]),
    };

    if let Some(selected_entry) = selected_entry {
        let entry_data = Paragraph::new(vec![
            Line::from(vec![
                Span::styled(
                    &selected_entry.name,
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::raw(" ["),
                Span::styled(
                    format!("{}", selected_entry.time_added.format("%x %I:%M %p")),
                    Style::default().add_modifier(Modifier::ITALIC),
                ),
                Span::raw("]"),
            ]),
            Line::from(Span::raw("")),
            Line::from(Span::raw(selected_entry.link.as_str())),
        ]);
        f.render_widget(entry_data, bottom_chunks[0]);
    }

    let key_options = app
        .select_state
        .get_options(app)
        .into_iter()
        .map(|opt| opt.to_line())
        .collect::<Vec<_>>();

    let key_par = Paragraph::new(key_options);

    f.render_widget(key_par, bottom_chunks[1]);
}
