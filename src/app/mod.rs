mod commands;
mod console;
mod navigation;

use crate::app::console::ConsoleWidget;
use crossterm::event::{DisableMouseCapture, EnableMouseCapture, Event, KeyCode};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::{event, ExecutableCommand};
use ratatui::prelude::*;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};
use std::io;
use std::io::{stdout, Stdout};
use std::time::{Duration, Instant};

pub struct App {
    console: ConsoleWidget,
}

pub trait Drawable {
    fn draw(&self, frame: &mut Frame, area: Rect);
}

impl App {
    pub fn new() -> Self {
        Self {
            console: Default::default(),
        }
    }

    pub fn run(&mut self) -> io::Result<()> {
        let mut terminal = init_terminal()?;
        let mut app = App::new();
        let mut last_tick = Instant::now();
        let tick_rate = Duration::from_millis(16);
        loop {
            let _ = terminal.draw(|frame| app.ui(frame));
            let timeout = tick_rate.saturating_sub(last_tick.elapsed());
            if event::poll(timeout)? {
                let event = event::read()?;
                if let Event::Key(key) = &event {
                    match key.code {
                        KeyCode::Char('q') => break,
                        _ => {}
                    }
                }
                app.on_event(event);
            }

            let update = Instant::now();
            if update - last_tick >= tick_rate {
                app.on_tick(update);
                last_tick = Instant::now();
            }
        }
        restore_terminal()
    }

    fn ui(&mut self, frame: &mut Frame) {
        let main_layout = Layout::new()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(80), Constraint::Percentage(20)])
            .split(frame.size());

        self.console.draw(frame, main_layout[0]);
        frame.render_widget(Block::new().borders(Borders::ALL).title("Status Bar"), main_layout[1]);
    }

    fn on_init(&mut self) {
        self.console.set_title("Console");
    }

    fn on_event(&mut self, event: Event) {
        if !matches!(event, Event::Resize(..)) {
            self.console.writeln(format_args!("Event: {event:?}"));
        }
    }

    fn on_tick(&mut self, time: Instant) {}
}

fn init_terminal() -> io::Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    Terminal::new(CrosstermBackend::new(stdout()))
}

fn restore_terminal() -> io::Result<()> {
    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;
    Ok(())
}
