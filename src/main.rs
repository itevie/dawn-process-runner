mod config;
mod keybinds;
mod managed_process;
mod util;

use std::{
    io::{self},
    time::Duration,
};

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};

use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    prelude::{Line, Stylize},
    style::{Modifier, Style},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};

use crate::config::load_config;
use crate::keybinds::{Keybind, KeybindContext, KeybindType, get_keybinds};
use crate::managed_process::ManagedProcess;
use crate::util::{format_duration, keycode_display};
use ratatui::prelude::Span;
use std::panic;
use std::process::{exit};
use std::sync::atomic::{AtomicBool, Ordering};

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

fn cleanup(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) {
    disable_raw_mode().ok();

    execute!(terminal.backend_mut(), LeaveAlternateScreen).ok();
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
                        let selected = app.selected();
                        app.processes[selected].restart();
                    }

                    KeybindType::Stop => {
                        let selected = app.selected();
                        app.processes[selected].stop();
                    }

                    KeybindType::Start => {
                        let selected = app.selected();
                        app.processes[selected].start();
                    }

                    KeybindType::Enter => app.view = View::Logs,

                    KeybindType::Quit => app.view = View::QuitConfirm,

                    _ => {}
                }
            }
        },

        View::QuitConfirm => {
            match code {
                KeyCode::Char('y') => {
                    RUNNING.store(false, Ordering::SeqCst);
                }

                KeyCode::Char('n') | KeyCode::Esc => {
                    app.view = View::List;
                }

                _ => {}
            }
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
    let config = match load_config("config.toml") {
        Ok(ok) => ok,
        Err(err) => {
            println!("Failed to load config: {}", err.to_string());
            exit(1);
        }
    };

    // Panic safety hook
    panic::set_hook(Box::new(|_| {
        disable_raw_mode().ok();
    }));

    enable_raw_mode()?;

    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(
        config
            .processes
            .iter()
            .map(|x| ManagedProcess::new(&x.name, x.cmd.clone(), x.cwd.clone()))
            .collect(),
    );

    for p in &mut app.processes {
        p.start();
    }

    while RUNNING.load(Ordering::SeqCst) {
        terminal.draw(|f| {
            let size = f.area();

            match app.view {
                View::List => {
                    let items: Vec<ListItem> = app
                        .processes
                        .iter_mut()
                        .map(|p| {
                            let status = p.status();
                            ListItem::new(format!(
                                "{} [{}{} {}]",
                                p.name,
                                status,
                                if let Some(status) = p.status {
                                    format!(" (code {})", status.code().unwrap_or(1))
                                } else {
                                    "".to_string()
                                },
                                if let Some(started_at) = p.started_at {
                                    format_duration(started_at.elapsed())
                                } else {
                                    "0s".to_string()
                                }
                            ))
                            .style(Style::default())
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
                        spans.push(format!("<{}>", keycode_display(&bind.0)).blue().bold())
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
                        .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
                        .highlight_symbol(">> ");

                    f.render_stateful_widget(list, size, &mut app.state);
                }

                View::Logs => {
                    let selected = app.selected();

                    let logs = app.processes[selected].logs.lock().unwrap().clone();

                    let paragraph = Paragraph::new(logs.join("\n"))
                        .block(Block::default().title("Logs (ESC)").borders(Borders::ALL))
                        .scroll((app.log_scroll, 0));

                    f.render_widget(paragraph, size);
                },

                View::QuitConfirm => {
                    let prompt = Paragraph::new(
                        "Quit program? (y/n)"
                    )
                        .block(
                            Block::default()
                                .title("Confirm Exit")
                                .borders(Borders::ALL)
                        )
                        .style(Style::default().add_modifier(Modifier::BOLD));

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

    cleanup(&mut terminal);

    Ok(())
}
