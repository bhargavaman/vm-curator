//! Virtual disk picker used when a VM has more than one system disk.

use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};
use std::path::PathBuf;

use crate::app::{App, Screen};

use super::super::centered_rect;

pub fn render(app: &App, frame: &mut Frame) {
    let Screen::ResizeDiskPicker {
        disk_paths,
        selected,
        ..
    } = &app.screen
    else {
        return;
    };

    render_disk_picker(disk_paths, *selected, frame);
}

fn render_disk_picker(disk_paths: &[PathBuf], selected: usize, frame: &mut Frame) {
    let area = frame.area();
    let dialog_width = 90.min(area.width.saturating_sub(4));
    let dialog_height = (disk_paths.len() as u16 + 8)
        .min(22)
        .min(area.height.saturating_sub(4));
    let dialog_area = centered_rect(dialog_width, dialog_height, area);
    frame.render_widget(Clear, dialog_area);

    let block = Block::default()
        .title(" Select Disk to Resize ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .style(Style::default().bg(Color::Black));
    let inner = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(1),
            Constraint::Length(2),
        ])
        .split(inner);

    let instructions = Paragraph::new(
        "This VM has multiple virtual system disks. Select the image whose capacity you want to increase.",
    )
    .style(Style::default().fg(Color::Gray))
    .wrap(Wrap { trim: true });
    frame.render_widget(instructions, chunks[0]);

    let items = disk_paths
        .iter()
        .enumerate()
        .map(|(index, path)| {
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("Disk {}  ", index + 1),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::raw(path.display().to_string()),
            ]))
        })
        .collect::<Vec<_>>();
    let list = List::new(items).highlight_style(
        Style::default()
            .fg(Color::Yellow)
            .bg(Color::DarkGray)
            .add_modifier(Modifier::BOLD),
    );
    let mut state = ListState::default();
    state.select(Some(selected));
    frame.render_stateful_widget(list, chunks[1], &mut state);

    let help = Paragraph::new("[j/k or ↑/↓] Navigate  [Enter] Select  [Esc] Cancel")
        .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(help, chunks[2]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{backend::TestBackend, Terminal};

    #[test]
    fn picker_renders_exact_disk_paths_and_controls() {
        let disk_paths = vec![
            PathBuf::from("/vms/test-vm/os.raw"),
            PathBuf::from("/vms/test-vm/data.qcow2"),
        ];
        let backend = TestBackend::new(100, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| render_disk_picker(&disk_paths, 1, frame))
            .unwrap();

        let buffer = terminal.backend().buffer();
        let rendered = buffer
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(rendered.contains("Select Disk to Resize"));
        assert!(rendered.contains("Disk 1  /vms/test-vm/os.raw"));
        assert!(rendered.contains("Disk 2  /vms/test-vm/data.qcow2"));
        assert!(rendered.contains("[Enter] Select"));
        assert!(rendered.contains("[Esc] Cancel"));
        assert!(buffer.content().iter().any(|cell| {
            cell.fg == Color::Yellow
                && cell.bg == Color::DarkGray
                && cell.modifier.contains(Modifier::BOLD)
        }));
    }

    #[test]
    fn picker_handles_an_empty_disk_list_without_panicking() {
        let backend = TestBackend::new(60, 12);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| render_disk_picker(&[], 0, frame))
            .unwrap();

        let rendered = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(rendered.contains("Select Disk to Resize"));
        assert!(rendered.contains("[Esc] Cancel"));
    }
}
