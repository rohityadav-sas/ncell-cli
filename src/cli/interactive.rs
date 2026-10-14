mod completion;

use super::{Args, Command, Runtime, run_with, script};
use clap::Parser;
use completion::{Completer, Completion, Suggestion};
use crossterm::{
    cursor::{Hide, MoveTo, MoveToColumn, MoveUp, Show},
    event::{
        self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyCode, KeyEvent, KeyEventKind,
        KeyModifiers,
    },
    execute, queue,
    style::ResetColor,
    terminal::{self, BeginSynchronizedUpdate, Clear, ClearType, EndSynchronizedUpdate},
};
use ncell_cli::{
    auth::AuthService,
    error::{Error, Result},
    session::{KeyringStore, SessionStore},
};
use std::{
    io::{self, IsTerminal, Write},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

const TEAL: &str = "132;202;190";
const LAVENDER: &str = "190;174;242";
const MUTED: &str = "137;145;161";
const BLUE: &str = "142;193;235";
const BORDER: &str = "86;77;109";
const PROMPT: &str = "  ❯ ";
const CATEGORY_TTL: Duration = Duration::from_secs(600);
const PACK_TTL: Duration = Duration::from_secs(300);

fn paint(color: &str, value: &str) -> String {
    // Terminal control sequences must never come from API text, paths or pasted input.
    let text: String = value.chars().filter(|ch| !ch.is_control()).collect();
    if colors_enabled() {
        format!("\x1b[38;2;{color}m{text}\x1b[0m")
    } else {
        text
    }
}
fn colors_enabled() -> bool {
    std::env::var_os("NO_COLOR").is_none_or(|value| value.is_empty())
}
fn selection(value: &str) -> String {
    if colors_enabled() {
        format!(
            "\x1b[48;2;53;45;73m{}\x1b[0m",
            paint(LAVENDER, value).replace("\x1b[0m", "")
        )
    } else {
        value.to_owned()
    }
}
fn width(value: &str) -> usize {
    UnicodeWidthStr::width(value)
}
fn clipped(value: &str, available: usize) -> String {
    let mut used = 0;
    value
        .chars()
        .filter(|ch| !ch.is_control())
        .take_while(|ch| {
            used += ch.width().unwrap_or(0);
            used <= available
        })
        .collect()
}

pub(super) fn welcome(columns: usize) -> String {
    let letters = [
        [
            "██    ██",
            "███   ██",
            "████  ██",
            "██ ██ ██",
            "██  ████",
            "██   ███",
            "██    ██",
        ],
        [
            " ██████ ",
            "██      ",
            "██      ",
            "██      ",
            "██      ",
            "██      ",
            " ██████ ",
        ],
        [
            "███████",
            "██     ",
            "██     ",
            "██████ ",
            "██     ",
            "██     ",
            "███████",
        ],
        [
            "██     ",
            "██     ",
            "██     ",
            "██     ",
            "██     ",
            "██     ",
            "███████",
        ],
        [
            "██     ",
            "██     ",
            "██     ",
            "██     ",
            "██     ",
            "██     ",
            "███████",
        ],
    ];
    let colors = [TEAL, "146;198;207", "160;190;224", "175;182;235", LAVENDER];
    if columns < 8 {
        return clipped("NCELL", columns.saturating_sub(1));
    }
    let inner = 53.min(columns.saturating_sub(4));
    let indent = " ".repeat(columns.saturating_sub(inner + 2) / 2);
    let frame = |value: String, visible: usize| {
        let left = inner.saturating_sub(visible) / 2;
        format!(
            "{indent}{}{}{value}{}{}",
            paint(BORDER, "│"),
            " ".repeat(left),
            " ".repeat(inner.saturating_sub(visible + left)),
            paint(BORDER, "│")
        )
    };
    let mut rows = vec![
        String::new(),
        format!(
            "{indent}{}",
            paint(BORDER, &format!("╭{}╮", "─".repeat(inner)))
        ),
        frame(String::new(), 0),
    ];
    if columns >= 53 {
        for row in 0..7 {
            let art = letters
                .iter()
                .zip(colors)
                .map(|(letter, color)| paint(color, letter[row]))
                .collect::<Vec<_>>()
                .join("  ");
            rows.push(frame(art, 45));
        }
    } else {
        rows.push(frame(paint(LAVENDER, "NCELL"), 5));
    }
    rows.push(frame(String::new(), 0));
    let subtitle = clipped("interactive · live commands", inner);
    rows.push(frame(paint(TEAL, &subtitle), width(&subtitle)));
    rows.push(frame(String::new(), 0));
    rows.push(format!(
        "{indent}{}",
        paint(BORDER, &format!("╰{}╯", "─".repeat(inner)))
    ));
    rows.push(String::new());
    let help = "Ctrl+Space suggest   ·   Tab / Enter accept";
    if columns >= width(help) {
        rows.push(format!(
            "{}{}{}{}{}",
            " ".repeat((columns - width(help)) / 2),
            paint(BLUE, "Ctrl+Space"),
            paint(MUTED, " suggest   ·   "),
            paint(BLUE, "Tab / Enter"),
            paint(MUTED, " accept")
        ));
    } else {
        // Narrow terminals retain one line rather than wrapping into the prompt.
        let compact = clipped("Ctrl+Space · Tab/Enter", columns.saturating_sub(1));
        rows.push(format!(
            "{}{}",
            " ".repeat(columns.saturating_sub(width(&compact)) / 2),
            paint(BLUE, &compact)
        ));
    }
    rows.push(String::new());
    rows.join("\n")
}

type PanicHook = Arc<dyn Fn(&std::panic::PanicHookInfo<'_>) + Send + Sync>;
struct TerminalGuard {
    active: bool,
    previous_hook: Option<PanicHook>,
}
impl TerminalGuard {
    fn enter() -> io::Result<Self> {
        let previous_hook: PanicHook = std::panic::take_hook().into();
        let panic_hook = previous_hook.clone();
        std::panic::set_hook(Box::new(move |info| {
            let _ = terminal::disable_raw_mode();
            let _ = execute!(io::stdout(), ResetColor, Show, DisableBracketedPaste);
            panic_hook(info);
        }));
        let mut guard = Self {
            active: false,
            previous_hook: Some(previous_hook),
        };
        guard.resume()?;
        Ok(guard)
    }
    fn resume(&mut self) -> io::Result<()> {
        terminal::enable_raw_mode()?;
        self.active = true;
        execute!(io::stdout(), EnableBracketedPaste, Show)
    }
    fn pause(&mut self) -> io::Result<()> {
        if self.active {
            terminal::disable_raw_mode()?;
            self.active = false;
            execute!(io::stdout(), ResetColor, Show, DisableBracketedPaste)?;
        }
        Ok(())
    }
}
impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = self.pause();
        if let Some(previous) = self.previous_hook.take()
            && !std::thread::panicking()
        {
            std::panic::set_hook(Box::new(move |info| previous(info)));
        }
    }
}

enum CacheEvent {
    Progress { completed: usize, total: usize },
    Finished(std::result::Result<Vec<Suggestion>, String>),
}

#[derive(Default)]
struct CategoryCache {
    values: Vec<Suggestion>,
    loaded: Option<Instant>,
    pending: Option<mpsc::Receiver<CacheEvent>>,
    progress: Option<(usize, usize)>,
    error: Option<String>,
}
impl CategoryCache {
    fn invalidate(&mut self) {
        *self = Self::default();
    }
    fn poll(&mut self) -> bool {
        let mut changed = false;
        let mut finished = None;
        let mut disconnected = false;
        if let Some(receiver) = &self.pending {
            loop {
                match receiver.try_recv() {
                    Ok(CacheEvent::Progress { completed, total }) => {
                        self.progress = Some((completed, total));
                        changed = true;
                    }
                    Ok(CacheEvent::Finished(result)) => {
                        finished = Some(result);
                        break;
                    }
                    Err(mpsc::TryRecvError::Disconnected) => {
                        disconnected = true;
                        break;
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                }
            }
        }
        if let Some(result) = finished {
            self.pending = None;
            self.progress = None;
            match result {
                Ok(values) => {
                    self.values = values;
                    self.loaded = Some(Instant::now());
                }
                Err(error) => self.error = Some(error),
            }
            return true;
        }
        if disconnected {
            self.pending = None;
            self.progress = None;
            self.error = Some("Could not load categories; press Ctrl+Space to retry".into());
            return true;
        }
        changed
    }
    fn fresh(&self, pack_mode: bool) -> bool {
        self.loaded.is_some_and(|loaded| {
            loaded.elapsed() < if pack_mode { PACK_TTL } else { CATEGORY_TTL }
        })
    }
    fn load(&mut self, runtime: &mut Runtime, pack_mode: bool) -> bool {
        if self.pending.is_some() || self.error.is_some() || self.fresh(pack_mode) {
            return false;
        }
        self.loaded = None;
        self.values.clear();
        let client = match runtime.client() {
            Ok(client) => client,
            Err(error) => {
                self.error = Some(error.to_string());
                return true;
            }
        };
        let (sender, receiver) = mpsc::channel();
        self.pending = Some(receiver);
        // Background suggestions refresh an expired access token exactly like
        // foreground commands, but never hold the account lock across slow
        // requests and rotate at most once: snapshot the tokens, fetch freely,
        // and on a 401/LGN2001 rotate once, re-snapshot, and retry.
        std::thread::spawn(move || {
            let snapshot = || -> std::result::Result<(String, String), String> {
                let store = KeyringStore::new().map_err(|error| error.to_string())?;
                let account = store
                    .load()
                    .map_err(|error| error.to_string())?
                    .and_then(|session| session.account)
                    .ok_or_else(|| Error::NotLoggedIn.to_string())?;
                Ok((account.msisdn, account.access_token))
            };
            let rotate_once = || -> std::result::Result<(), String> {
                let store = KeyringStore::new().map_err(|error| error.to_string())?;
                AuthService::new(client.clone(), store)
                    .refresh()
                    .map_err(|error| error.to_string())
            };
            let result = (|| {
                let (msisdn, token) = snapshot()?;
                let response = match client.categories(&msisdn, &token) {
                    Err(Error::Api {
                        status: 401, code, ..
                    }) if code == "LGN2001" => {
                        rotate_once()?;
                        let (msisdn, token) = snapshot()?;
                        client
                            .categories(&msisdn, &token)
                            .map_err(|error| error.to_string())?
                    }
                    response => response.map_err(|error| error.to_string())?,
                };
                if pack_mode {
                    let categories = completion::supported_category_ids(&response)?;
                    let total = categories.len();
                    let _ = sender.send(CacheEvent::Progress {
                        completed: 0,
                        total,
                    });
                    // Plans use fresh snapshot tokens with no locks held, so
                    // concurrent fetches never block each other or the UI.
                    let (msisdn, token) = snapshot()?;
                    let fetched = std::thread::scope(|scope| {
                        let completed = Arc::new(AtomicUsize::new(0));
                        let handles = categories
                            .into_iter()
                            .map(|category| {
                                let client = client.clone();
                                let msisdn = msisdn.clone();
                                let token = token.clone();
                                let completed = Arc::clone(&completed);
                                let sender = sender.clone();
                                scope.spawn(move || {
                                    let result = client
                                        .plans(&msisdn, &token, category)
                                        .map(|response| (category, response))
                                        .map_err(|error| error.to_string());
                                    let completed = completed.fetch_add(1, Ordering::Relaxed) + 1;
                                    let _ = sender.send(CacheEvent::Progress { completed, total });
                                    result
                                })
                            })
                            .collect::<Vec<_>>();
                        handles
                            .into_iter()
                            .map(|handle| {
                                handle
                                    .join()
                                    .map_err(|_| "Could not load packs".to_owned())?
                            })
                            .collect::<std::result::Result<Vec<_>, String>>()
                    })?;
                    completion::merge_fetched_packs(fetched)
                } else {
                    completion::categories(&response)
                }
            })();
            let _ = sender.send(CacheEvent::Finished(result));
        });
        true
    }
}

#[derive(Default)]
struct Editor {
    input: String,
    cursor: usize,
    selected: usize,
    explicit: bool,
    dismissed: bool,
    all_selected: bool,
    message: Option<String>,
}
#[derive(Debug, PartialEq, Eq)]
enum Action {
    Continue,
    Submit,
    Clear,
    Exit,
}
/// Env-gated trace (NCELL_TRACE=1) for diagnosing interactive dispatch.
/// Appends markers to %TEMP%\ncell-trace.log. Never enabled in normal runs.
fn trace(marker: &str) {
    if std::env::var_os("NCELL_TRACE").is_none() {
        return;
    }
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(std::env::temp_dir().join("ncell-trace.log"))
    {
        use std::io::Write as _;
        let _ = writeln!(file, "{marker}");
    }
}
/// Blank every visible row with spaces, then park at the top-left.
/// Uses only cursor moves and spaces so it works even where erase
/// sequences are ignored. Scrollback above the viewport is preserved.
fn clear_viewport() -> io::Result<()> {
    let (columns, rows) = terminal::size().unwrap_or((80, 24));
    let blank: String = " ".repeat(columns as usize);
    let mut output = io::stdout();
    for row in 0..rows {
        queue!(output, MoveTo(0, row))?;
        write!(output, "{blank}")?;
    }
    queue!(output, MoveTo(0, 0))?;
    output.flush()
}
fn suggestion_shortcut(key: KeyEvent) -> bool {
    matches!(key.code, KeyCode::Null | KeyCode::Char('\0'))
        || (key.modifiers.contains(KeyModifiers::CONTROL)
            && (matches!(key.code, KeyCode::Char(' ' | '@' | '`'))
                || (key.modifiers.contains(KeyModifiers::SHIFT) && key.code == KeyCode::Char('2'))))
}
fn previous_boundary(input: &str, cursor: usize) -> usize {
    input[..cursor]
        .char_indices()
        .next_back()
        .map_or(0, |(index, _)| index)
}
fn next_boundary(input: &str, cursor: usize) -> usize {
    input[cursor..]
        .chars()
        .next()
        .map_or(cursor, |ch| cursor + ch.len_utf8())
}

impl Editor {
    fn completion(&self, completer: &Completer, categories: &[Suggestion]) -> Completion {
        let mut completion =
            completer.complete(&self.input, self.cursor, self.explicit, categories);
        if self.dismissed || self.all_selected {
            completion.suggestions.clear();
            completion.needs_categories = false;
            completion.needs_packs = false;
        }
        completion
    }
    fn insert(&mut self, value: &str) {
        if self.all_selected {
            self.input.clear();
            self.cursor = 0;
            self.all_selected = false;
        }
        self.input.insert_str(self.cursor, value);
        self.cursor += value.len();
        self.selected = 0;
        self.dismissed = false;
        self.explicit = false;
    }
    fn accept(&mut self, completion: &Completion, item: &Suggestion) {
        self.input
            .replace_range(completion.replace.clone(), &item.value);
        self.cursor = completion.replace.start + item.value.len();
        if let Some(category) = item.subscription_category {
            let option = format!(" --category {category}");
            self.input.insert_str(self.cursor, &option);
            self.cursor += option.len();
        }
        let directory = item.description == "Directory";
        if item.append_space && !self.input[self.cursor..].starts_with(char::is_whitespace) {
            self.input.insert(self.cursor, ' ');
            self.cursor += 1;
        }
        self.selected = 0;
        self.explicit = item.open_subcommands;
        self.dismissed = !self.explicit && !directory;
        self.all_selected = false;
    }
    fn key(&mut self, key: KeyEvent, completion: &Completion) -> Action {
        if key.kind == KeyEventKind::Release {
            return Action::Continue;
        }
        self.message = None;
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if ctrl && matches!(key.code, KeyCode::Char('c' | 'd')) {
            return Action::Exit;
        }
        if suggestion_shortcut(key) {
            self.explicit = true;
            self.dismissed = false;
            self.selected = 0;
            self.all_selected = false;
            return Action::Continue;
        }
        if ctrl && key.code == KeyCode::Char('a') {
            self.all_selected = !self.input.is_empty();
            self.cursor = self.input.len();
            self.dismissed = true;
            return Action::Continue;
        }
        if key.code == KeyCode::Esc {
            self.dismissed = true;
            self.explicit = false;
            self.selected = 0;
            self.all_selected = false;
            return Action::Continue;
        }
        let selected = completion.suggestions.get(
            self.selected
                .min(completion.suggestions.len().saturating_sub(1)),
        );
        match key.code {
            KeyCode::Up | KeyCode::Down if !completion.suggestions.is_empty() => {
                let length = completion.suggestions.len();
                self.selected = (self.selected
                    + if key.code == KeyCode::Up {
                        length - 1
                    } else {
                        1
                    })
                    % length;
            }
            KeyCode::Tab => {
                if let Some(item) = selected {
                    self.accept(completion, item);
                } else {
                    self.explicit = true;
                    self.dismissed = false;
                    self.selected = 0;
                }
            }
            KeyCode::Enter => {
                if self.input.trim() == "exit" || self.input.trim() == "ncell exit" {
                    trace("key-exit");
                    return Action::Exit;
                }
                if self.input.trim() == "clear" || self.input.trim() == "ncell clear" {
                    trace("key-clear");
                    return Action::Clear;
                }
                if let Some(item) = selected
                    && (self.input[completion.replace.clone()] != item.value || item.continue_input)
                {
                    self.accept(completion, item);
                    return Action::Continue;
                }
                if !self.input.trim().is_empty() {
                    return Action::Submit;
                }
            }
            KeyCode::Backspace if ctrl => self.delete_word(),
            KeyCode::Char('w') if ctrl => self.delete_word(),
            KeyCode::Backspace | KeyCode::Delete if self.all_selected => {
                self.input.clear();
                self.cursor = 0;
                self.all_selected = false;
                self.changed();
            }
            KeyCode::Backspace if self.cursor > 0 => {
                let start = previous_boundary(&self.input, self.cursor);
                self.input.replace_range(start..self.cursor, "");
                self.cursor = start;
                self.changed();
            }
            KeyCode::Delete if self.cursor < self.input.len() => {
                self.input
                    .replace_range(self.cursor..next_boundary(&self.input, self.cursor), "");
                self.changed();
            }
            KeyCode::Home => {
                self.cursor = 0;
                self.all_selected = false;
                self.dismissed = false;
            }
            KeyCode::End => {
                self.cursor = self.input.len();
                self.all_selected = false;
                self.dismissed = false;
            }
            KeyCode::Left => {
                self.cursor = if self.all_selected {
                    0
                } else if ctrl {
                    word_start(&self.input, self.cursor)
                } else {
                    previous_boundary(&self.input, self.cursor)
                };
                self.all_selected = false;
                self.dismissed = false;
            }
            KeyCode::Right => {
                self.cursor = if self.all_selected {
                    self.input.len()
                } else if ctrl {
                    word_end(&self.input, self.cursor)
                } else {
                    next_boundary(&self.input, self.cursor)
                };
                self.all_selected = false;
                self.dismissed = false;
            }
            KeyCode::Char('e') if ctrl => {
                self.cursor = self.input.len();
                self.all_selected = false;
            }
            KeyCode::Char(ch)
                if !ctrl && !key.modifiers.contains(KeyModifiers::ALT) && !ch.is_control() =>
            {
                self.insert(&ch.to_string())
            }
            _ => (),
        }
        Action::Continue
    }
    fn changed(&mut self) {
        self.selected = 0;
        self.dismissed = false;
        self.explicit = false;
    }
    fn delete_word(&mut self) {
        let start = if self.all_selected {
            0
        } else {
            word_start(&self.input, self.cursor)
        };
        if self.all_selected {
            self.input.clear();
        } else {
            self.input.replace_range(start..self.cursor, "");
        }
        self.cursor = start;
        self.all_selected = false;
        self.changed();
    }
    fn submitted(&mut self) {
        self.input.clear();
        self.cursor = 0;
        self.selected = 0;
        self.explicit = false;
        self.dismissed = false;
        self.all_selected = false;
        self.message = None;
    }
}
fn word_start(input: &str, mut cursor: usize) -> usize {
    while cursor > 0
        && input[..cursor]
            .chars()
            .next_back()
            .is_some_and(char::is_whitespace)
    {
        cursor = previous_boundary(input, cursor);
    }
    while cursor > 0
        && input[..cursor]
            .chars()
            .next_back()
            .is_some_and(|ch| !ch.is_whitespace())
    {
        cursor = previous_boundary(input, cursor);
    }
    cursor
}
fn word_end(input: &str, mut cursor: usize) -> usize {
    while cursor < input.len()
        && input[cursor..]
            .chars()
            .next()
            .is_some_and(char::is_whitespace)
    {
        cursor = next_boundary(input, cursor);
    }
    while cursor < input.len()
        && input[cursor..]
            .chars()
            .next()
            .is_some_and(|ch| !ch.is_whitespace())
    {
        cursor = next_boundary(input, cursor);
    }
    cursor
}

fn command_colors(input: &str) -> String {
    let mut position = 0;
    input
        .split_inclusive(char::is_whitespace)
        .map(|token| {
            let color = if token.trim_start().starts_with('-') {
                BLUE
            } else if token.trim_start().starts_with(['\'', '"']) {
                TEAL
            } else if token.chars().next().is_some_and(|ch| ch.is_ascii_digit())
                || token.contains(":asc")
                || token.contains(":desc")
                || token.trim_end().ends_with(".ncell")
            {
                "224;192;137"
            } else if position < 2 {
                LAVENDER
            } else {
                "208;208;216"
            };
            if !token.trim().is_empty() {
                position += 1;
            }
            paint(color, token)
        })
        .collect()
}

fn viewport(input: &str, cursor: usize, available: usize) -> (usize, String) {
    let mut start = 0;
    while width(&input[start..cursor]) >= available && start < cursor {
        start = next_boundary(input, start);
    }
    (start, clipped(&input[start..], available))
}

fn wrap_notice(message: &str, available: usize) -> Vec<String> {
    // Word-wrap long notices across the reserved empty rows instead of cutting
    // them off; the box keeps its fixed size so nothing shifts while typing.
    let mut lines = Vec::new();
    let mut line = String::new();
    let mut line_width = 0;
    for word in message.split_whitespace() {
        let word_width = width(word);
        let gap = if line.is_empty() { 0 } else { 1 };
        if line_width + gap + word_width <= available {
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(word);
            line_width += gap + word_width;
        } else {
            if !line.is_empty() {
                lines.push(std::mem::take(&mut line));
                line_width = 0;
            }
            if word_width <= available {
                line = word.to_owned();
                line_width = word_width;
            } else {
                // Hard-cut a single overlong word by display cells.
                let mut piece = String::new();
                let mut piece_width = 0;
                for ch in word.chars() {
                    let w = ch.width().unwrap_or(0);
                    if piece_width + w > available && !piece.is_empty() {
                        lines.push(std::mem::take(&mut piece));
                        piece_width = 0;
                    }
                    piece.push(ch);
                    piece_width += w;
                }
                if !piece.is_empty() {
                    lines.push(piece);
                }
            }
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

fn dropdown(
    completion: &Completion,
    selected: usize,
    input: &str,
    offset: usize,
    columns: usize,
    available_rows: usize,
) -> Vec<String> {
    if (completion.suggestions.is_empty() && !completion.needs_categories)
        || available_rows < 3
        || columns < 12
    {
        return Vec::new();
    }
    let selected = selected.min(completion.suggestions.len().saturating_sub(1));
    let slots = if completion.needs_categories {
        10.min(available_rows.saturating_sub(2))
    } else {
        completion
            .suggestions
            .len()
            .min(available_rows.saturating_sub(2))
    };
    let shown = completion.suggestions.len().min(slots);
    let start = selected.saturating_sub(shown.saturating_sub(1));
    let items = &completion.suggestions[start..start + shown];
    let label_width = completion
        .suggestions
        .iter()
        .map(|item| width(&item.value))
        .max()
        .unwrap_or(0);
    let description_width = completion
        .suggestions
        .iter()
        .map(|item| width(&item.description))
        .max()
        .unwrap_or(0);
    let desired = if completion.needs_categories {
        64
    } else {
        (label_width + description_width + 7).max(20)
    };
    let anchor = width(PROMPT)
        + if completion.replace.start >= offset {
            width(&input[offset..completion.replace.start])
        } else {
            0
        };
    let anchored_width = columns.saturating_sub(anchor);
    let box_width = desired
        .min(64)
        .min(if anchored_width >= 20 {
            anchored_width
        } else {
            columns.saturating_sub(2)
        })
        .max(8);
    let left = anchor.min(columns.saturating_sub(box_width));
    let indent = " ".repeat(left);
    let inner = box_width - 2;
    let mut rows = vec![format!(
        "{indent}{}",
        paint(BORDER, &format!("╭{}╮", "─".repeat(inner)))
    )];
    let label_limit = label_width.min(inner.saturating_sub(5));
    // Wrap the empty-state notice across the reserved rows instead of cutting
    // it off; only the first slots are used when there are no suggestions.
    let wrapped_notice: Vec<String> = if completion.suggestions.is_empty() {
        wrap_notice(
            completion
                .category_status
                .as_deref()
                .unwrap_or(if completion.needs_packs {
                    "No matching packs."
                } else {
                    "No matching categories."
                }),
            inner.saturating_sub(2),
        )
    } else {
        Vec::new()
    };
    // Pack rows carry the price inside the description ("Name · Rs. 22.00").
    // Split it into its own column so prices line up across rows.
    let priced: Vec<(&str, &str)> = completion
        .suggestions
        .iter()
        .map(|item| match item.description.rfind(" · Rs. ") {
            Some(at) => (
                &item.description[..at],
                &item.description[at + " · ".len()..],
            ),
            None => (item.description.as_str(), ""),
        })
        .collect();
    let price_width = priced
        .iter()
        .map(|(_, price)| width(price))
        .max()
        .unwrap_or(0);
    let head_width = priced
        .iter()
        .map(|(head, _)| width(head))
        .max()
        .unwrap_or(0);
    for (index, item) in items.iter().enumerate() {
        let label = clipped(&item.value, label_limit);
        let rest = inner.saturating_sub(label_limit + 5);
        let (head_text, price_text) = priced[start + index];
        let (head, gap, price) = if price_width > 0 && rest >= price_width + 2 {
            let price = if price_text.is_empty() {
                String::new()
            } else {
                clipped(price_text, price_width)
            };
            let head_col = head_width.min(rest.saturating_sub(if price.is_empty() {
                0
            } else {
                price_width + 2
            }));
            let head = clipped(head_text, head_col);
            let gap = if price.is_empty() {
                String::new()
            } else {
                " ".repeat(head_col.saturating_sub(width(&head)) + 2)
            };
            (head, gap, price)
        } else {
            (
                clipped(&item.description, rest),
                String::new(),
                String::new(),
            )
        };
        let price_pad = if price.is_empty() {
            String::new()
        } else {
            " ".repeat(price_width.saturating_sub(width(&price)))
        };
        let tail = format!("{head}{gap}{price_pad}{price}");
        let mut tail_colored = paint(MUTED, &head);
        if !price.is_empty() {
            tail_colored.push_str(&gap);
            tail_colored.push_str(&price_pad);
            tail_colored.push_str(&paint(MUTED, &price));
        }
        let marker = if start + index == selected && !colors_enabled() {
            '❯'
        } else {
            item.kind
        };
        let text = format!(
            " {} {}{}{}",
            marker,
            label,
            " ".repeat(label_limit.saturating_sub(width(&label)) + 2),
            tail
        );
        let text = clipped(&text, inner);
        let padded = format!("{text}{}", " ".repeat(inner.saturating_sub(width(&text))));
        let content = if start + index == selected {
            selection(&padded)
        } else {
            format!(
                " {} {}{}{}{}",
                paint(TEAL, &item.kind.to_string()),
                paint(LAVENDER, &label),
                " ".repeat(label_limit.saturating_sub(width(&label)) + 2),
                tail_colored,
                " ".repeat(inner.saturating_sub(width(&text)))
            )
        };
        rows.push(format!(
            "{indent}{}{content}{}",
            paint(BORDER, "│"),
            paint(BORDER, "│")
        ));
    }
    for index in shown..slots {
        let status = wrapped_notice
            .get(index - shown)
            .map(String::as_str)
            .unwrap_or("");
        let text = clipped(&format!("  {status}"), inner);
        rows.push(format!(
            "{indent}{}{}{}{}",
            paint(BORDER, "│"),
            paint(MUTED, &text),
            " ".repeat(inner.saturating_sub(width(&text))),
            paint(BORDER, "│")
        ));
    }
    let footer = if completion.needs_categories && !completion.suggestions.is_empty() {
        clipped(
            &format!(
                " {}–{} of {} · ↑↓ more ",
                start + 1,
                start + shown,
                completion.suggestions.len()
            ),
            inner,
        )
    } else {
        String::new()
    };
    rows.push(format!(
        "{indent}{}",
        paint(
            BORDER,
            &format!(
                "╰{}{}╯",
                footer,
                "─".repeat(inner.saturating_sub(width(&footer)))
            )
        )
    ));
    rows
}

fn draw(editor: &Editor, completion: &Completion, cache: &CategoryCache) -> io::Result<()> {
    let (columns, height) = terminal::size().unwrap_or((80, 24));
    let columns = usize::from(columns).saturating_sub(1).max(1);
    let prompt = clipped(PROMPT, columns.saturating_sub(1));
    let prompt_width = width(&prompt);
    let available = columns.saturating_sub(prompt_width).max(1);
    let (offset, displayed) = viewport(&editor.input, editor.cursor, available);
    let mut hints = Vec::new();
    if let Some(hint) = &completion.hint {
        hints.push(format!(
            "{}{}",
            " ".repeat(prompt_width),
            paint(
                if completion.sort_hint { MUTED } else { TEAL },
                &clipped(hint, available)
            )
        ));
    }
    let mut panel = completion.clone();
    if panel.needs_categories || panel.needs_packs {
        panel.needs_categories = true;
        panel.category_status = if cache.pending.is_some() {
            Some(if panel.needs_packs {
                cache.progress.map_or_else(
                    || "Loading packs…".to_owned(),
                    |(completed, total)| format!("Loading packs · {completed}/{total} categories"),
                )
            } else {
                "Loading categories…".to_owned()
            })
        } else {
            cache.error.clone()
        };
    }
    if let Some(message) = &editor.message {
        hints.push(format!(
            "{}{}",
            " ".repeat(prompt_width),
            paint("232;179;115", &clipped(message, available))
        ));
    }
    hints.truncate(usize::from(height).saturating_sub(2));
    let mut rows = dropdown(
        &panel,
        editor.selected,
        &editor.input,
        offset,
        columns,
        usize::from(height).saturating_sub(hints.len() + 2),
    );
    rows.extend(hints);
    // Paint the whole frame into a buffer and flush it in one write inside a
    // synchronized update, so the terminal never shows a half-painted frame.
    let mut frame: Vec<u8> = Vec::with_capacity(4096);
    queue!(
        frame,
        BeginSynchronizedUpdate,
        Hide,
        MoveToColumn(0),
        Clear(ClearType::FromCursorDown)
    )?;
    write!(
        frame,
        "{}{}",
        paint(TEAL, &prompt),
        if editor.all_selected {
            selection(&displayed)
        } else {
            command_colors(&displayed)
        }
    )?;
    for row in &rows {
        write!(frame, "\r\n{row}")?;
    }
    if !rows.is_empty() {
        queue!(frame, MoveUp(rows.len() as u16))?;
    }
    queue!(
        frame,
        MoveToColumn((prompt_width + width(&editor.input[offset..editor.cursor])) as u16),
        Show,
        EndSynchronizedUpdate
    )?;
    let mut output = io::stdout().lock();
    output.write_all(&frame)?;
    output.flush()
}

fn parse_line(input: &str) -> std::result::Result<Args, ParseError> {
    let mut words = script::words(input).map_err(ParseError::Message)?;
    if words.first().is_some_and(|word| word == "ncell") {
        words.remove(0);
    }
    let args = Args::try_parse_from(std::iter::once("ncell".to_owned()).chain(words)).map_err(
        |error| {
            if matches!(
                error.kind(),
                clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
            ) {
                ParseError::Display(error.to_string())
            } else {
                ParseError::Message(
                    error
                        .to_string()
                        .lines()
                        .next()
                        .unwrap_or("Invalid command")
                        .trim_start_matches("error: ")
                        .to_owned(),
                )
            }
        },
    )?;
    if matches!(args.command, Command::Interactive) {
        return Err(ParseError::Message(
            "Already in interactive mode; type exit to return to your shell".into(),
        ));
    }
    super::validate_args(&args).map_err(ParseError::Message)?;
    Ok(args)
}
enum ParseError {
    Message(String),
    Display(String),
}
fn fail(error: io::Error) -> Error {
    Error::Script(format!("Interactive terminal: {error}"))
}

pub(super) fn run(runtime: &mut Runtime, json: bool) -> Result<()> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err(Error::InvalidInput(
            "Interactive mode requires a terminal; use ordinary ncell commands for pipes and scripts",
        ));
    }
    if json {
        return Err(Error::InvalidInput(
            "Use --json on commands inside interactive mode",
        ));
    }
    let mut guard = TerminalGuard::enter().map_err(fail)?;
    // Fresh viewport on entry so the first frame matches a cleared screen.
    // Spaces keep the outer shell in the scrollback instead of erasing it.
    clear_viewport().map_err(fail)?;
    println!(
        "{}",
        welcome(usize::from(terminal::size().unwrap_or((80, 24)).0))
    );
    let mut editor = Editor::default();
    let completer = Completer::new();
    let mut categories = CategoryCache::default();
    let mut packs = CategoryCache::default();
    let mut dirty = true;
    loop {
        dirty |= categories.poll();
        dirty |= packs.poll();
        let mut completion = editor.completion(&completer, &categories.values);
        if completion.needs_packs {
            dirty |= packs.load(runtime, true);
            completion.suggestions = completion::pack_suggestions(
                &packs.values,
                &completion.pack_query,
                completion.pack_category,
            );
        }
        if completion.needs_categories {
            dirty |= categories.load(runtime, false);
        }
        if dirty {
            draw(
                &editor,
                &completion,
                if completion.needs_packs {
                    &packs
                } else {
                    &categories
                },
            )
            .map_err(fail)?;
            dirty = false;
        }
        if !event::poll(Duration::from_millis(100)).map_err(fail)? {
            continue;
        }
        let event = event::read().map_err(fail)?;
        if matches!(
            event,
            Event::Key(KeyEvent {
                kind: KeyEventKind::Release,
                ..
            })
        ) {
            continue;
        }
        dirty = true;
        let action = match event {
            Event::Key(key) => {
                if categories.error.is_some() && suggestion_shortcut(key) {
                    categories.invalidate();
                }
                if packs.error.is_some() && suggestion_shortcut(key) {
                    packs.invalidate();
                }
                editor.key(key, &completion)
            }
            Event::Paste(text) => {
                let text = text
                    .chars()
                    .map(|ch| if ch.is_whitespace() { ' ' } else { ch })
                    .filter(|ch| !ch.is_control())
                    .collect::<String>();
                editor.insert(&text);
                Action::Continue
            }
            _ => Action::Continue,
        };
        if action == Action::Continue {
            continue;
        }
        if action == Action::Exit {
            clear_viewport().map_err(fail)?;
            guard.pause().map_err(fail)?;
            return Ok(());
        }
        if action == Action::Clear {
            trace("branch-clear");
            clear_viewport().map_err(fail)?;
            println!(
                "{}",
                welcome(usize::from(terminal::size().unwrap_or((80, 24)).0))
            );
            trace("branch-clear-painted");
            editor.submitted();
            continue;
        }
        let parsed = match parse_line(&editor.input) {
            Err(ParseError::Message(message)) => {
                editor.message = Some(message);
                continue;
            }
            value => value,
        };
        queue!(
            io::stdout(),
            MoveToColumn(0),
            Clear(ClearType::FromCursorDown)
        )
        .map_err(fail)?;
        guard.pause().map_err(fail)?;
        println!("{}{}\n", paint(TEAL, PROMPT), command_colors(&editor.input));
        match parsed {
            Ok(args) => {
                let json = args.json;
                let invalidate = matches!(
                    &args.command,
                    Command::Login { .. }
                        | Command::Account { .. }
                        | Command::Logout
                        | Command::Refresh
                        | Command::Script { .. }
                );
                let subscribed = matches!(
                    &args.command,
                    Command::Plans {
                        action: Some(super::PlanCommand::Subscribe { .. }),
                        ..
                    }
                );
                if let Err(error) = run_with(args, runtime) {
                    if json {
                        if let Error::Api { response, .. }
                        | Error::CheckIn { response, .. }
                        | Error::GameTasks { response } = &error
                        {
                            println!("{response:#}");
                        } else {
                            println!("{:#}", serde_json::json!({"error":error.to_string()}));
                        }
                    } else if !matches!(error, Error::GameLost) {
                        eprintln!("Error: {error}");
                    }
                }
                if invalidate {
                    categories.invalidate();
                    packs.invalidate();
                } else if subscribed {
                    packs.invalidate();
                }
            }
            Err(ParseError::Display(output)) => print!("{output}"),
            Err(ParseError::Message(_)) => unreachable!(),
        }
        println!();
        editor.submitted();
        // Do not interpret the Enter/keyup left by an OTP or external editor as a new command.
        while event::poll(Duration::ZERO).map_err(fail)? {
            let _ = event::read().map_err(fail)?;
        }
        guard.resume().map_err(fail)?;
    }
}

#[cfg(test)]
#[path = "../../tests/unit/interactive.rs"]
mod tests;
