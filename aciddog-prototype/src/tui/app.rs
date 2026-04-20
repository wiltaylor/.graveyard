use std::io;

use anyhow::Result;
use crossterm::event::{Event, EventStream, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use futures::StreamExt;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tracing::{debug, info};

use crate::config::AciddogConfig;

use super::ui;

pub struct App {
    pub config: AciddogConfig,
    pub selected: usize,
    pub should_quit: bool,
}

impl App {
    pub fn new(config: AciddogConfig) -> Self {
        Self {
            config,
            selected: 0,
            should_quit: false,
        }
    }

    fn select_next(&mut self) {
        if self.config.agents.is_empty() {
            return;
        }
        self.selected = (self.selected + 1) % self.config.agents.len();
    }

    fn select_prev(&mut self) {
        if self.config.agents.is_empty() {
            return;
        }
        if self.selected == 0 {
            self.selected = self.config.agents.len() - 1;
        } else {
            self.selected -= 1;
        }
    }

    fn handle_key(&mut self, code: KeyCode, mods: KeyModifiers) {
        match (code, mods) {
            (KeyCode::Char('q'), _) | (KeyCode::Esc, _) => {
                info!("quit requested");
                self.should_quit = true;
            }
            (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                info!("ctrl-c received");
                self.should_quit = true;
            }
            (KeyCode::Char('j') | KeyCode::Down, _) => self.select_next(),
            (KeyCode::Char('k') | KeyCode::Up, _) => self.select_prev(),
            _ => {}
        }
    }
}

#[tracing::instrument(skip(config))]
pub async fn run(config: AciddogConfig) -> Result<()> {
    info!(team = %config.team.id, agents = config.agents.len(), "starting TUI");

    let mut stdout = io::stdout();
    enable_raw_mode()?;
    execute!(stdout, EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let mut app = App::new(config);

    let result = event_loop(&mut terminal, &mut app).await;

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    info!("TUI exited");
    result
}

async fn event_loop<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
) -> Result<()> {
    let mut events = EventStream::new();
    terminal.draw(|f| ui::render(f, app))?;

    while let Some(event) = events.next().await {
        match event? {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                debug!(?key, "key press");
                app.handle_key(key.code, key.modifiers);
            }
            Event::Resize(_, _) => {
                debug!("resize");
            }
            _ => {}
        }

        if app.should_quit {
            break;
        }

        terminal.draw(|f| ui::render(f, app))?;
    }

    Ok(())
}
