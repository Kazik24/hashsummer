use crate::app::Drawable;
use ratatui::layout::Rect;
use ratatui::prelude::*;
use ratatui::text::Line;
use ratatui::widgets::block::Title;
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;
use std::fmt::Arguments;
use std::io::Lines;

#[derive(Default)]
pub struct ConsoleWidget {
    pub title: Title<'static>,
    pub lines: Vec<Line<'static>>,
    pub scroll: u16,
}

impl ConsoleWidget {
    pub fn set_title(&mut self, title: &str) {
        self.title.content = title.to_string().into();
    }

    pub fn writeln(&mut self, args: Arguments<'_>) {
        self.lines.push(Line::from(format!("{args}\n")));
    }

    pub fn clear(&mut self) {
        self.lines.clear();
    }
}

impl Drawable for ConsoleWidget {
    fn draw(&self, frame: &mut Frame, area: Rect) {
        let line_count = area.height.saturating_sub(2).min(self.lines.len() as _) as usize;
        let lines = self.lines[(self.lines.len() - line_count)..].to_vec();
        let p = Paragraph::new(Text::from(lines))
            .scroll((self.scroll, 0))
            .block(Block::new().title(self.title.clone()).borders(Borders::ALL));
        frame.render_widget(p, area);
    }
}
