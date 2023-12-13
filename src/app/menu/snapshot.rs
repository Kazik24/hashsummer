use crate::app::Drawable;
use ratatui::layout::Rect;
use ratatui::Frame;

/// +---------------------------------------------+
/// |  Path:
/// |  >| ...                             |
/// |
/// |  |Change Settings|
/// |  |Estimate|
/// |
/// |  Snapshot file:
/// |  >|                                 |
/// |  |Start snapshot|
/// +---------------------------------------------+
pub struct SnapshotMenu {}

impl Drawable for SnapshotMenu {
    fn draw(&self, frame: &mut Frame, area: Rect) {
        todo!()
    }
}
