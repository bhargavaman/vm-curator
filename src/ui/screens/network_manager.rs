//! Virtual Network Manager screen (issue #53).
//!
//! Lists managed networks with live Active/Inactive status, edits their
//! definitions, and starts/stops them by running the generated
//! `net-up.sh`/`net-down.sh` scripts with sudo in a terminal window — the
//! TUI itself never modifies host networking.

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout},
    prelude::*,
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
};

use crate::app::{App, ConfirmAction, Screen, VNetEditorState};
use crate::vnet::{self, VirtualNetwork};

pub fn render(app: &App, frame: &mut Frame) {
    let area = frame.area();
    let dialog_width = 78.min(area.width.saturating_sub(4));
    let dialog_height = 24.min(area.height.saturating_sub(4));
    let dialog_area = centered_rect(dialog_width, dialog_height, area);
    frame.render_widget(Clear, dialog_area);

    let block = Block::default()
        .title(format!(" Networks ({} defined) ", app.vnet_networks.len()))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .style(Style::default().bg(Color::Black));
    let inner = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    let h_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(1),
            Constraint::Length(2),
        ])
        .split(inner);

    const LIST_HELP: &str =
        "[c] Create  [e] Edit  [d] Delete  [Enter] Start/Stop  [r] Refresh  [Esc] Back";
    // Reserve exactly the rows the help needs at this width so a narrow
    // terminal wraps it instead of clipping the last hints (#100).
    let help_rows = wrapped_rows(LIST_HELP, h_chunks[1].width);

    let v_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),         // Header/intro
            Constraint::Length(1),         // Spacer
            Constraint::Min(4),            // Network list
            Constraint::Length(4),         // Notes
            Constraint::Length(help_rows), // Help
        ])
        .split(h_chunks[1]);

    let intro = Paragraph::new("Managed virtual networks (Linux bridges owned by vm-curator).")
        .style(Style::default().fg(Color::Yellow));
    frame.render_widget(intro, v_chunks[0]);

    if app.vnet_networks.is_empty() {
        let empty = Paragraph::new("No networks defined yet. Press [c] to create one.")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
        frame.render_widget(empty, v_chunks[2]);
    } else {
        let items: Vec<ListItem> = app
            .vnet_networks
            .iter()
            .enumerate()
            .map(|(i, net)| {
                let selected = i == app.vnet_selected;
                let name_style = if selected {
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                let active = net.is_active();
                let (status, status_style) = if active {
                    ("● Active  ", Style::default().fg(Color::Green))
                } else {
                    ("○ Inactive", Style::default().fg(Color::DarkGray))
                };
                let dhcp = if net.dhcp { "DHCP" } else { "    " };
                ListItem::new(Line::from(vec![
                    Span::styled(format!("{:<13}", net.name), name_style),
                    Span::styled(
                        format!("{:<9}", net.kind.label()),
                        Style::default().fg(Color::Cyan),
                    ),
                    Span::styled(format!("{:<19}", net.subnet), name_style),
                    Span::styled(format!("{:<5}", dhcp), Style::default().fg(Color::DarkGray)),
                    Span::styled(status, status_style),
                ]))
            })
            .collect();

        let mut state = ListState::default();
        state.select(Some(app.vnet_selected));
        let list = List::new(items).highlight_symbol("> ");
        frame.render_stateful_widget(list, v_chunks[2], &mut state);
    }

    let notes = Paragraph::new(
        "Start/stop runs the network's generated net-up.sh / net-down.sh with\n\
         sudo in a terminal window (inspect them under ~/.config/vm-curator/\n\
         networks/). Attach VMs via Network Settings → bridge.",
    )
    .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(notes, v_chunks[3]);

    let help = Paragraph::new(LIST_HELP)
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });
    frame.render_widget(help, v_chunks[4]);

    // Create/edit form overlays the list
    if let Some(editor) = &app.vnet_editor {
        render_editor(editor, frame);
    }
}

/// Help shown under the form while a text field is being typed into.
const EDITOR_HELP_TYPING: &str = "[Enter] Done  [Esc] Cancel edit";
/// Help shown under the form otherwise. Two lines so the way to finish the
/// form is never the part that gets clipped on a narrow terminal (#100).
const EDITOR_HELP: &str =
    "[j/k/Tab] Field  [Enter] Edit/toggle  [←/→] Change\n[s] Save  [Esc] Cancel";

/// Rows needed to show `text` wrapped at `width` columns (word-wrap
/// estimate: each line of `text` costs ceil(chars / width), minimum one).
/// Width 0 counts as one row per line so layouts stay sane while resizing.
fn wrapped_rows(text: &str, width: u16) -> u16 {
    let width = width.max(1) as usize;
    text.lines()
        .map(|line| {
            let chars = line.chars().count();
            if chars == 0 {
                1
            } else {
                chars.div_ceil(width)
            }
        })
        .sum::<usize>()
        .max(1) as u16
}

/// Rows of the form, in order, inside the editor dialog's inner area.
struct EditorLayout {
    fields: [Rect; 4],
    actions: Rect,
    info: Rect,
    help: Rect,
}

/// Split the editor's inner area so the fields come first, the help is
/// anchored to the bottom with the rows it needs at this width, and the
/// info/error text takes whatever is left (shrinking first when the terminal
/// is short). Nothing the user must see is dropped before the info text.
fn editor_layout(inner: Rect, help_rows: u16) -> EditorLayout {
    // One-column side margin matches the previous look.
    let x = inner.x + 1;
    let width = inner.width.saturating_sub(2);
    // Rows past the bottom collapse to zero height so nothing is ever drawn
    // outside the dialog (ratatui panics on out-of-buffer rects).
    let row = |y: u16| {
        if y < inner.bottom() {
            Rect::new(x, y, width, 1)
        } else {
            Rect::new(x, inner.bottom(), width, 0)
        }
    };

    let mut y = inner.y;
    let fields = [row(y), row(y + 1), row(y + 2), row(y + 3)];
    y += 4;
    // Spacer, then the button row.
    y += 1;
    let actions = row(y);
    y += 2; // button row + spacer

    let help_rows = help_rows.min(inner.height);
    let help_y = inner.bottom().saturating_sub(help_rows);
    let help = Rect::new(x, help_y, width, help_rows);

    let info_h = help_y.saturating_sub(y);
    let info = Rect::new(x, y, width, info_h);

    EditorLayout {
        fields,
        actions,
        info,
        help,
    }
}

fn render_editor(editor: &VNetEditorState, frame: &mut Frame) {
    let area = frame.area();
    let dialog_width = 64.min(area.width.saturating_sub(4));
    let dialog_height = 17.min(area.height.saturating_sub(2));
    let dialog_area = centered_rect(dialog_width, dialog_height, area);
    frame.render_widget(Clear, dialog_area);

    let title = if editor.original_name.is_some() {
        " Edit Network "
    } else {
        " Create Network "
    };
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow))
        .style(Style::default().bg(Color::Black));
    let inner = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    let help_text = if editor.editing {
        EDITOR_HELP_TYPING
    } else {
        EDITOR_HELP
    };
    let layout = editor_layout(
        inner,
        wrapped_rows(help_text, inner.width.saturating_sub(2)),
    );

    let field = |label: &str, value: String, focus: usize| -> Line<'static> {
        let focused = editor.field_focus == focus;
        let editing_this = focused && editor.editing;
        let shown = if editing_this {
            format!("{}_", editor.edit_buffer)
        } else {
            value
        };
        Line::from(vec![
            Span::styled(
                if focused { "> " } else { "  " }.to_string(),
                Style::default().fg(Color::Yellow),
            ),
            Span::styled(format!("{:<9}", label), Style::default().fg(Color::Yellow)),
            Span::styled(
                shown,
                if editing_this {
                    Style::default().fg(Color::Cyan)
                } else if focused {
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                },
            ),
        ])
    };

    let name_value = if editor.original_name.is_some() {
        format!("{} (fixed)", editor.name)
    } else {
        editor.name.clone()
    };
    frame.render_widget(
        Paragraph::new(field("Name:", name_value, VNetEditorState::FIELD_NAME)),
        layout.fields[0],
    );
    frame.render_widget(
        Paragraph::new(field(
            "Type:",
            format!("[ {} ]  (←/→ toggle)", editor.kind.label()),
            VNetEditorState::FIELD_TYPE,
        )),
        layout.fields[1],
    );
    frame.render_widget(
        Paragraph::new(field(
            "Subnet:",
            editor.subnet.clone(),
            VNetEditorState::FIELD_SUBNET,
        )),
        layout.fields[2],
    );
    frame.render_widget(
        Paragraph::new(field(
            "DHCP:",
            format!("[{}]", if editor.dhcp { "on" } else { "off" }),
            VNetEditorState::FIELD_DHCP,
        )),
        layout.fields[3],
    );

    // Button row: a visible way to finish the form (#100). `s`/`Esc` still work.
    let on_actions = editor.field_focus == VNetEditorState::FIELD_ACTIONS;
    let button = |label: &str, active: bool| -> Span<'static> {
        let style = if on_actions && active {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else if on_actions {
            Style::default().fg(Color::White)
        } else {
            Style::default().fg(Color::Gray)
        };
        Span::styled(format!("[ {} ]", label), style)
    };
    let actions = Line::from(vec![
        Span::styled(
            if on_actions { "> " } else { "  " }.to_string(),
            Style::default().fg(Color::Yellow),
        ),
        Span::styled(format!("{:<9}", ""), Style::default()),
        button("Save", !editor.action_cancel),
        Span::raw("   "),
        button("Cancel", editor.action_cancel),
    ]);
    frame.render_widget(Paragraph::new(actions), layout.actions);

    let info = if let Some(err) = &editor.error {
        Paragraph::new(err.clone()).style(Style::default().fg(Color::Red))
    } else {
        Paragraph::new(
            "Gateway and DHCP range are derived from the subnet.\n\
             NAT = outbound internet; Isolated = host-only.",
        )
        .style(Style::default().fg(Color::DarkGray))
    };
    frame.render_widget(info.wrap(Wrap { trim: true }), layout.info);

    let help = Paragraph::new(help_text)
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });
    frame.render_widget(help, layout.help);
}

pub fn handle_key(app: &mut App, key: KeyEvent) -> Result<()> {
    if app.vnet_editor.is_some() {
        return handle_editor_key(app, key);
    }

    match key.code {
        KeyCode::Esc => {
            app.pop_screen();
        }
        KeyCode::Char('j') | KeyCode::Down => {
            if !app.vnet_networks.is_empty() && app.vnet_selected + 1 < app.vnet_networks.len() {
                app.vnet_selected += 1;
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            if app.vnet_selected > 0 {
                app.vnet_selected -= 1;
            }
        }
        KeyCode::Char('c') | KeyCode::Char('C') => {
            app.vnet_editor = Some(VNetEditorState::new_network());
        }
        KeyCode::Char('e') | KeyCode::Char('E') => {
            if let Some(net) = app.vnet_networks.get(app.vnet_selected) {
                if net.is_active() {
                    app.set_status(
                        "Stop the network before editing (its scripts must match host state)",
                    );
                } else {
                    app.vnet_editor = Some(VNetEditorState::edit(net));
                }
            }
        }
        KeyCode::Char('d') | KeyCode::Char('D') => {
            if let Some(net) = app.vnet_networks.get(app.vnet_selected) {
                if net.is_active() {
                    app.set_status("Stop the network before deleting it");
                } else {
                    app.push_screen(Screen::Confirm(ConfirmAction::DeleteNetwork));
                }
            }
        }
        KeyCode::Char('r') | KeyCode::Char('R') => {
            app.reload_vnet_networks();
            app.set_status("Refreshed networks");
        }
        KeyCode::Enter | KeyCode::Char(' ') => {
            if let Some(net) = app.vnet_networks.get(app.vnet_selected).cloned() {
                start_or_stop(app, &net);
            }
        }
        _ => {}
    }
    Ok(())
}

/// What the editor wants the app to do after a key (pure form logic is in
/// [`apply_editor_key`] so it can be tested without an `App`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EditorAction {
    None,
    Save,
    Cancel,
}

fn handle_editor_key(app: &mut App, key: KeyEvent) -> Result<()> {
    let action = match app.vnet_editor.as_mut() {
        Some(editor) => apply_editor_key(editor, key.code),
        None => return Ok(()),
    };
    match action {
        EditorAction::None => {}
        EditorAction::Save => save_editor(app),
        EditorAction::Cancel => app.vnet_editor = None,
    }
    Ok(())
}

/// Apply a key to the form state and report whether it should save or close.
fn apply_editor_key(editor: &mut VNetEditorState, code: KeyCode) -> EditorAction {
    if editor.editing {
        match code {
            KeyCode::Esc => {
                editor.editing = false;
                editor.edit_buffer.clear();
            }
            KeyCode::Enter | KeyCode::Tab => {
                let value = editor.edit_buffer.trim().to_string();
                match editor.field_focus {
                    VNetEditorState::FIELD_NAME => editor.name = value,
                    VNetEditorState::FIELD_SUBNET => editor.subnet = value,
                    _ => {}
                }
                editor.editing = false;
                editor.edit_buffer.clear();
            }
            KeyCode::Backspace => {
                editor.edit_buffer.pop();
            }
            KeyCode::Char(c) if !c.is_whitespace() => {
                editor.edit_buffer.push(c);
            }
            _ => {}
        }
        return EditorAction::None;
    }

    match code {
        KeyCode::Esc => return EditorAction::Cancel,
        KeyCode::Char('s') | KeyCode::Char('S') => return EditorAction::Save,
        KeyCode::Char('j') | KeyCode::Down => {
            if editor.field_focus + 1 < VNetEditorState::FIELD_COUNT {
                editor.field_focus += 1;
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            if editor.field_focus > 0 {
                editor.field_focus -= 1;
            }
        }
        // Tab walks the rows (wrapping), so from the DHCP toggle the next Tab
        // lands on the Save button.
        KeyCode::Tab => {
            editor.field_focus = (editor.field_focus + 1) % VNetEditorState::FIELD_COUNT;
        }
        KeyCode::BackTab => {
            editor.field_focus = (editor.field_focus + VNetEditorState::FIELD_COUNT - 1)
                % VNetEditorState::FIELD_COUNT;
        }
        KeyCode::Left | KeyCode::Right => match editor.field_focus {
            VNetEditorState::FIELD_TYPE => editor.kind = editor.kind.toggle(),
            VNetEditorState::FIELD_DHCP => editor.dhcp = !editor.dhcp,
            VNetEditorState::FIELD_ACTIONS => editor.action_cancel = !editor.action_cancel,
            _ => {}
        },
        KeyCode::Enter => match editor.field_focus {
            VNetEditorState::FIELD_NAME if editor.original_name.is_none() => {
                editor.editing = true;
                editor.edit_buffer = editor.name.clone();
            }
            VNetEditorState::FIELD_SUBNET => {
                editor.editing = true;
                editor.edit_buffer = editor.subnet.clone();
            }
            VNetEditorState::FIELD_TYPE => editor.kind = editor.kind.toggle(),
            VNetEditorState::FIELD_DHCP => editor.dhcp = !editor.dhcp,
            VNetEditorState::FIELD_ACTIONS => {
                return if editor.action_cancel {
                    EditorAction::Cancel
                } else {
                    EditorAction::Save
                };
            }
            _ => {}
        },
        _ => {}
    }
    EditorAction::None
}

/// Build the network from the form, save it, and close the editor.
fn save_editor(app: &mut App) {
    let Some(editor) = app.vnet_editor.as_mut() else {
        return;
    };

    // Creating: reject duplicate names
    if editor.original_name.is_none() && app.vnet_networks.iter().any(|n| n.name == editor.name) {
        editor.error = Some(format!("A network named '{}' already exists", editor.name));
        return;
    }

    let built =
        VirtualNetwork::with_defaults(&editor.name, editor.kind, &editor.subnet).map(|mut net| {
            net.dhcp = editor.dhcp && net.dhcp;
            net
        });
    match built {
        Ok(net) => {
            if let Err(e) = vnet::save_network(&vnet::networks_dir(), &net) {
                editor.error = Some(format!("Save failed: {e}"));
                return;
            }
            let name = net.name.clone();
            app.vnet_editor = None;
            app.reload_vnet_networks();
            if let Some(pos) = app.vnet_networks.iter().position(|n| n.name == name) {
                app.vnet_selected = pos;
            }
            app.set_status(format!("Saved network '{name}'"));
        }
        Err(e) => {
            editor.error = Some(e.to_string());
        }
    }
}

/// Run the appropriate script in a terminal with sudo.
fn start_or_stop(app: &mut App, net: &VirtualNetwork) {
    let active = net.is_active();
    let script = vnet::script_path(&vnet::networks_dir(), &net.name, !active);
    if !script.exists() {
        app.set_status(format!("Script missing: {}", script.display()));
        return;
    }
    let action = if active { "Stopping" } else { "Starting" };
    match run_script_in_terminal(&script) {
        Ok(()) => app.set_status(format!(
            "{action} '{}' in a terminal window — authorize sudo there, then press r to refresh",
            net.name
        )),
        Err(e) => app.set_status(e),
    }
}

/// Spawn `sudo <script>` in the first available terminal emulator
/// (same launcher list as the single-GPU passthrough setup).
fn run_script_in_terminal(script: &std::path::Path) -> std::result::Result<(), String> {
    let script_path = script.to_string_lossy().to_string();
    let terminals: &[(&str, &[&str])] = &[
        ("alacritty", &["-e", "sudo"]),
        ("kitty", &["sudo"]),
        ("ghostty", &["-e", "sudo"]),
        ("gnome-terminal", &["--", "sudo"]),
        ("konsole", &["-e", "sudo"]),
        ("xfce4-terminal", &["-x", "sudo"]),
        ("xterm", &["-e", "sudo"]),
    ];

    for (term, args) in terminals {
        let found = std::process::Command::new("which")
            .arg(term)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if !found {
            continue;
        }
        let mut cmd = std::process::Command::new(term);
        cmd.args(*args).arg(&script_path);
        if cmd.spawn().is_ok() {
            return Ok(());
        }
    }
    Err(
        "No terminal found. Install alacritty, kitty, ghostty, konsole, or gnome-terminal — \
         or run the script manually with sudo."
            .to_string(),
    )
}

/// Delete the selected network (called from the confirm dialog).
pub(crate) fn delete_selected(app: &mut App) {
    let Some(net) = app.vnet_networks.get(app.vnet_selected).cloned() else {
        return;
    };
    if net.is_active() {
        app.set_status("Network became active; stop it before deleting");
        return;
    }
    match vnet::delete_network(&vnet::networks_dir(), &net.name) {
        Ok(()) => {
            app.reload_vnet_networks();
            app.set_status(format!("Deleted network '{}'", net.name));
        }
        Err(e) => app.set_status(format!("Delete failed: {e}")),
    }
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    Rect::new(x, y, width, height)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vnet::VNetKind;

    // --- help-bar measurement (#100) ---

    #[test]
    fn wrapped_rows_fits_on_one_line_when_wide_enough() {
        assert_eq!(wrapped_rows("[s] Save  [Esc] Cancel", 80), 1);
    }

    #[test]
    fn wrapped_rows_grows_on_narrow_terminals() {
        // 22 chars at width 10 -> 3 rows
        assert_eq!(wrapped_rows("[s] Save  [Esc] Cancel", 10), 3);
        // Each line of a multi-line help is measured separately.
        assert_eq!(wrapped_rows(EDITOR_HELP, 80), 2);
        assert_eq!(wrapped_rows(EDITOR_HELP, 20), 3 + 2);
        assert_eq!(wrapped_rows("", 40), 1);
        assert_eq!(wrapped_rows("abc", 0), 3);
    }

    #[test]
    fn editor_help_fits_the_default_dialog_width() {
        // Dialog 64 wide, borders 2, side margin 2 -> 60 usable columns.
        for line in EDITOR_HELP.lines() {
            assert!(line.chars().count() <= 60, "help line too long: {line}");
        }
    }

    // --- layout: help is never the thing that gets dropped ---

    #[test]
    fn editor_layout_anchors_help_to_the_bottom() {
        let inner = Rect::new(2, 3, 62, 15);
        let l = editor_layout(inner, 2);
        assert_eq!(l.fields[0].y, 3);
        assert_eq!(l.fields[3].y, 6);
        assert_eq!(l.actions.y, 8);
        assert_eq!(l.help.height, 2);
        assert_eq!(l.help.bottom(), inner.bottom());
        assert_eq!(l.info.y, 10);
        assert_eq!(l.info.bottom(), l.help.y);
        assert!(l.info.height >= 2, "info keeps room on a normal dialog");
    }

    #[test]
    fn editor_layout_keeps_help_visible_on_a_short_terminal() {
        // The reporter's terminal: ~15 rows -> dialog 13 -> inner 11.
        let inner = Rect::new(0, 0, 50, 11);
        let l = editor_layout(inner, 2);
        assert_eq!(l.help.height, 2);
        assert_eq!(l.help.bottom(), 11);
        // Info is what gives way, not the buttons or help.
        assert_eq!(l.actions.y, 5);
        assert_eq!(l.info.height, 11 - 2 - 7);
        // Even absurdly short: nothing panics, help still has its rows, and
        // no row is placed outside the dialog.
        let tiny = editor_layout(Rect::new(0, 0, 50, 3), 2);
        assert_eq!(tiny.help.height, 2);
        assert_eq!(tiny.info.height, 0);
        assert!(tiny.actions.bottom() <= 3);
        assert!(tiny.fields.iter().all(|r| r.bottom() <= 3));
        assert_eq!(tiny.actions.height, 0);
    }

    // --- form key handling ---

    fn fresh() -> VNetEditorState {
        VNetEditorState::new_network()
    }

    #[test]
    fn focus_walks_down_to_the_button_row_and_stops() {
        let mut e = fresh();
        for _ in 0..10 {
            apply_editor_key(&mut e, KeyCode::Char('j'));
        }
        assert_eq!(e.field_focus, VNetEditorState::FIELD_ACTIONS);
        for _ in 0..10 {
            apply_editor_key(&mut e, KeyCode::Char('k'));
        }
        assert_eq!(e.field_focus, VNetEditorState::FIELD_NAME);
    }

    #[test]
    fn tab_wraps_from_dhcp_to_save_button() {
        let mut e = fresh();
        e.field_focus = VNetEditorState::FIELD_DHCP;
        assert_eq!(apply_editor_key(&mut e, KeyCode::Tab), EditorAction::None);
        assert_eq!(e.field_focus, VNetEditorState::FIELD_ACTIONS);
        apply_editor_key(&mut e, KeyCode::Tab);
        assert_eq!(e.field_focus, VNetEditorState::FIELD_NAME);
        apply_editor_key(&mut e, KeyCode::BackTab);
        assert_eq!(e.field_focus, VNetEditorState::FIELD_ACTIONS);
    }

    #[test]
    fn enter_on_save_button_saves_and_on_cancel_button_cancels() {
        let mut e = fresh();
        e.field_focus = VNetEditorState::FIELD_ACTIONS;
        assert!(!e.action_cancel, "Save is highlighted by default");
        assert_eq!(apply_editor_key(&mut e, KeyCode::Enter), EditorAction::Save);

        apply_editor_key(&mut e, KeyCode::Right);
        assert!(e.action_cancel);
        assert_eq!(
            apply_editor_key(&mut e, KeyCode::Enter),
            EditorAction::Cancel
        );

        apply_editor_key(&mut e, KeyCode::Left);
        assert!(!e.action_cancel);
    }

    #[test]
    fn s_and_esc_still_finish_the_form_from_any_field() {
        let mut e = fresh();
        e.field_focus = VNetEditorState::FIELD_SUBNET;
        assert_eq!(
            apply_editor_key(&mut e, KeyCode::Char('s')),
            EditorAction::Save
        );
        assert_eq!(apply_editor_key(&mut e, KeyCode::Esc), EditorAction::Cancel);
    }

    #[test]
    fn enter_on_fields_edits_or_toggles_without_saving() {
        let mut e = fresh();
        assert_eq!(apply_editor_key(&mut e, KeyCode::Enter), EditorAction::None);
        assert!(e.editing, "Enter on Name starts typing");
        for c in "lab".chars() {
            apply_editor_key(&mut e, KeyCode::Char(c));
        }
        assert_eq!(apply_editor_key(&mut e, KeyCode::Enter), EditorAction::None);
        assert!(!e.editing);
        assert_eq!(e.name, "lab");

        e.field_focus = VNetEditorState::FIELD_TYPE;
        apply_editor_key(&mut e, KeyCode::Enter);
        assert_eq!(e.kind, VNetKind::Isolated);
        apply_editor_key(&mut e, KeyCode::Right);
        assert_eq!(e.kind, VNetKind::Nat);

        e.field_focus = VNetEditorState::FIELD_DHCP;
        apply_editor_key(&mut e, KeyCode::Enter);
        assert!(!e.dhcp);
    }

    #[test]
    fn esc_while_typing_only_cancels_the_edit() {
        let mut e = fresh();
        apply_editor_key(&mut e, KeyCode::Enter);
        apply_editor_key(&mut e, KeyCode::Char('x'));
        assert_eq!(apply_editor_key(&mut e, KeyCode::Esc), EditorAction::None);
        assert!(!e.editing);
        assert_eq!(e.name, "", "buffer discarded, form still open");
    }

    #[test]
    fn name_is_fixed_when_editing_an_existing_network() {
        let net = VirtualNetwork::with_defaults("lab", VNetKind::Nat, "10.10.0.0/24").unwrap();
        let mut e = VNetEditorState::edit(&net);
        e.field_focus = VNetEditorState::FIELD_NAME;
        apply_editor_key(&mut e, KeyCode::Enter);
        assert!(!e.editing);
    }
}
