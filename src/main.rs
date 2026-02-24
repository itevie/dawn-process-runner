mod config;
mod keybinds;
mod managed_process;
mod util;

use std::{
    io,
    process::exit,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};

use ratatui::{
    backend::CrosstermBackend,
    prelude::{Line, Span, Stylize},
    style::{Modifier, Style},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Terminal,
};

use crate::config::load_config;
use crate::keybinds::{get_keybinds, Keybind, KeybindContext, KeybindType};
use crate::managed_process::ManagedProcess;
use crate::util::{format_duration, keycode_display};

static RUNNING: AtomicBool = AtomicBool::new(true);

enum View {
    List,
    Logs,
    QuitConfirm,
}

struct App {
    processes: Vec<ManagedProcess>,
    state: ListState,
    view: View,
    log_scroll: u16,
}

impl App {
    fn new(processes: Vec<ManagedProcess>) -> Self {
        let mut state = ListState::default();
        state.select(Some(0));

        Self {
            processes,
            state,
            view: View::List,
            log_scroll: 0,
        }
    }

    fn selected(&self) -> usize {
        self.state.selected().unwrap_or(0)
    }

    fn next(&mut self) {
        let i = self.selected();
        let next = if i + 1 >= self.processes.len() {
            0
        } else {
            i + 1
        };
        self.state.select(Some(next));
    }

    fn previous(&mut self) {
        let i = self.selected();
        let prev = if i == 0 {
            self.processes.len() - 1
        } else {
            i - 1
        };
        self.state.select(Some(prev));
    }
}

struct TerminalGuard {
    terminal: Terminal<CrosstermBackend<io::Stdout>>,
}

impl TerminalGuard {
    fn new() -> Result<Self, io::Error> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;

        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;

        Ok(Self { terminal })
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        disable_raw_mode().ok();
        execute!(self.terminal.backend_mut(), LeaveAlternateScreen).ok();
    }
}

fn handle_key(app: &mut App, code: KeyCode) {
    let keybinds = get_keybinds();

    match app.view {
        View::List => {
            if let Some(bind) = keybinds.get(&code) {
                match bind.t {
                    KeybindType::Down => app.next(),
                    KeybindType::Up => app.previous(),
                    KeybindType::Restart => {
                        let i = app.selected();
                        app.processes[i].restart();
                    }
                    KeybindType::Stop => {
                        let i = app.selected();
                        app.processes[i].stop();
                    }
                    KeybindType::Start => {
                        let i = app.selected();
                        app.processes[i].start();
                    }
                    KeybindType::Enter => app.view = View::Logs,
                    KeybindType::Quit => app.view = View::QuitConfirm,
                    _ => {}
                }
            }
        }

        View::QuitConfirm => match code {
            KeyCode::Char('y') => {
                RUNNING.store(false, Ordering::Relaxed);
            }
            KeyCode::Char('n') | KeyCode::Esc => {
                app.view = View::List;
            }
            _ => {}
        },

        View::Logs => {
            if let Some(bind) = keybinds.get(&code) {
                match bind.t {
                    KeybindType::Escape => app.view = View::List,
                    KeybindType::Up => {
                        if app.log_scroll > 0 {
                            app.log_scroll -= 1;
                        }
                    }
                    KeybindType::Down => app.log_scroll += 1,
                    _ => {}
                }
            }
        }
    }
}

fn main() -> Result<(), io::Error> {
    // ---- Ctrl+C handler ----
    ctrlc::set_handler(|| {
        RUNNING.store(false, Ordering::Relaxed);
    })
        .expect("Failed to set Ctrl-C handler");

    // ---- Load config ----
    let config = load_config("config.toml").unwrap_or_else(|e| {
        eprintln!("Failed to load config: {e}");
        exit(1);
    });

    // ---- Setup terminal (RAII safe) ----
    let mut guard = TerminalGuard::new()?;

    // ---- Build app ----
    let mut app = App::new(
        config
            .processes
            .iter()
            .map(|x| {
                ManagedProcess::new(
                    &x.name,
                    x.cmd.clone(),
                    x.cwd.clone(),
                    x.port,
                )
            })
            .collect(),
    );

    // ---- Start all processes ----
    for p in &mut app.processes {
        p.start();
    }

    // ---- Main event loop ----
    while RUNNING.load(Ordering::Relaxed) {
        guard.terminal.draw(|f| {
            let size = f.area();

            match app.view {
                View::List => {
                    let items: Vec<ListItem> = app
                        .processes
                        .iter_mut()
                        .map(|p| {
                            let status = p.status();
                            let exit_code = p
                                .exit_status
                                .and_then(|s| s.code())
                                .map(|c| format!(" (code {c})"))
                                .unwrap_or_default();

                            let runtime = p
                                .started_at
                                .map(|t| format_duration(t.elapsed()))
                                .unwrap_or_else(|| "0s".into());

                            ListItem::new(format!(
                                "{} [{}{} {}]",
                                p.name, status, exit_code, runtime
                            ))
                        })
                        .collect();

                    let mut binds = get_keybinds()
                        .iter()
                        .filter(|x| x.1.context == KeybindContext::Main)
                        .map(|x| (x.0.clone(), x.1.clone()))
                        .collect::<Vec<(KeyCode, Keybind)>>();

                    binds.sort_by(|a, b| a.1.name.cmp(&b.1.name));

                    let mut spans: Vec<Span> = vec![];

                    for bind in binds {
                        spans.push(format!(" {} ", bind.1.name).into());
                        spans.push(
                            format!("<{}>", keycode_display(&bind.0))
                                .blue()
                                .bold(),
                        );
                    }

                    spans.push(" ".into());

                    let instructions = Line::from(spans);

                    let list = List::new(items)
                        .block(
                            Block::default()
                                .title("Processes")
                                .title_bottom(instructions.centered())
                                .borders(Borders::ALL),
                        )
                        .highlight_style(
                            Style::default().add_modifier(Modifier::REVERSED),
                        )
                        .highlight_symbol(">> ");

                    f.render_stateful_widget(list, size, &mut app.state);
                }

                View::Logs => {
                    let selected = app.selected();

                    let text = {
                        let logs = app.processes[selected]
                            .logs
                            .lock()
                            .unwrap();

                        logs.iter().fold(String::new(), |mut acc, line| {
                            acc.push_str(line);
                            acc.push('\n');
                            acc
                        })
                    };

                    let paragraph = Paragraph::new(text)
                        .block(
                            Block::default()
                                .title("Logs (ESC)")
                                .borders(Borders::ALL),
                        )
                        .scroll((app.log_scroll, 0));

                    f.render_widget(paragraph, size);
                }

                View::QuitConfirm => {
                    let prompt = Paragraph::new("Quit program? (y/n)")
                        .block(
                            Block::default()
                                .title("Confirm Exit")
                                .borders(Borders::ALL),
                        )
                        .style(
                            Style::default().add_modifier(Modifier::BOLD),
                        );

                    f.render_widget(prompt, size);
                }
            }
        })?;

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    handle_key(&mut app, key.code);
                }
            }
        }
    }

    // ---- Clean shutdown (processes first) ----
    println!("Test");
    for p in &mut app.processes {
        println!("{}", p.name);
        p.stop();
    }

    // Terminal restored automatically via Drop
    println!();

    Ok(())
}