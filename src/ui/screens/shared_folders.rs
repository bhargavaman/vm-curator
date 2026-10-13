//! Shared Folders Screen
//!
//! Manages shared folders between host and guest VM — virtio-9p by default,
//! or QEMU's built-in SMB server for guests without a 9p driver (Windows).
//! Shows configured folders, mount instructions based on OS tier,
//! and allows adding/removing shared directories and switching modes.

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout},
    prelude::*,
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
};

use crate::app::{App, ConfirmAction, FileBrowserMode, Screen, UnsavedKind};
use crate::vm::{ShareMode, SMB_GUEST_SHARE};

/// Render the shared folders screen
pub fn render(app: &App, frame: &mut Frame) {
    let area = frame.area();

    let dialog_width = 70.min(area.width.saturating_sub(4));
    let dialog_height = 24.min(area.height.saturating_sub(4));

    let dialog_area = centered_rect(dialog_width, dialog_height, area);
    frame.render_widget(Clear, dialog_area);

    let folder_count = app.shared_folders.len();
    let title = format!(" Shared Folders ({} configured) ", folder_count);

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .style(Style::default().bg(Color::Black));

    let inner = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    // Add horizontal margins
    let h_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(2), // Left margin
            Constraint::Min(1),    // Content
            Constraint::Length(2), // Right margin
        ])
        .split(inner);

    // Split into padding, folder list, separator, instructions, help
    let v_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),                       // Top padding
            Constraint::Length(folder_list_height(app)), // Folder list
            Constraint::Length(1),                       // Separator
            Constraint::Min(4),                          // Mount instructions
            Constraint::Length(2),                       // Help text
        ])
        .split(h_chunks[1]);

    let list_area = v_chunks[1];
    let separator_area = v_chunks[2];
    let instructions_area = v_chunks[3];
    let help_area = v_chunks[4];

    // Render folder list
    if app.shared_folders.is_empty() {
        let empty_msg = Paragraph::new("No shared folders configured.")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
        frame.render_widget(empty_msg, list_area);
    } else {
        let items: Vec<ListItem> = app
            .shared_folders
            .iter()
            .enumerate()
            .map(|(i, folder)| {
                let style = if i == app.shared_folder_selected {
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };

                let detail = match folder.mode {
                    ShareMode::NineP => format!("  (9p, tag: {})", folder.mount_tag),
                    ShareMode::Smb => format!("  (SMB, {})", SMB_GUEST_SHARE),
                };

                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!("  {}. ", i + 1),
                        Style::default().fg(Color::DarkGray),
                    ),
                    Span::styled(&folder.host_path, style),
                    Span::styled(detail, Style::default().fg(Color::DarkGray)),
                ]))
            })
            .collect();

        let mut state = ListState::default();
        state.select(Some(app.shared_folder_selected));

        let list = List::new(items).highlight_symbol("> ");
        frame.render_stateful_widget(list, list_area, &mut state);
    }

    // Separator
    let sep = Paragraph::new("─".repeat(separator_area.width as usize))
        .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(sep, separator_area);

    // Mount instructions
    render_mount_instructions(app, frame, instructions_area);

    // Help text
    let help = Paragraph::new("[a] Add  [d] Remove  [m] 9p/SMB  [s] Save  [Esc] Back")
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);
    frame.render_widget(help, help_area);
}

/// Calculate the height needed for the folder list
fn folder_list_height(app: &App) -> u16 {
    let count = app.shared_folders.len();
    if count == 0 {
        2
    } else {
        (count as u16).min(6)
    }
}

/// Render mount instructions based on the OS tier.
///
/// 9p folders get the tier's mount commands (one line per mount tag); an SMB
/// folder gets the tier's SMB block (`smb_<tier>`, falling back to `smb`).
/// With both kinds configured, both blocks are shown.
fn render_mount_instructions(app: &App, frame: &mut Frame, area: Rect) {
    let content = mount_instructions_text(app);

    let paragraph = Paragraph::new(content)
        .style(Style::default().fg(Color::White))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

/// The mount-instructions panel text (pure, for testing).
pub(crate) fn mount_instructions_text(app: &App) -> String {
    if app.shared_folders.is_empty() {
        let (title, description) = app.shared_folders_help.get_or_default("empty_state");
        return format!("{}:\n{}", title, description.trim_end());
    }

    let tier = get_mount_tier(app);
    let nine_p_tags: Vec<&str> = app
        .shared_folders
        .iter()
        .filter(|f| f.mode == ShareMode::NineP)
        .map(|f| f.mount_tag.as_str())
        .collect();
    let smb_folder = app.shared_folders.iter().find(|f| f.mode == ShareMode::Smb);

    let mut blocks: Vec<String> = Vec::new();

    if !nine_p_tags.is_empty() {
        let (title, description) = app.shared_folders_help.get_or_default(tier);

        // Expand {TAG} placeholders: lines containing {TAG} are repeated once per
        // 9p folder with that folder's mount tag. Lines without {TAG} render once.
        let mut expanded = String::new();
        for line in description.lines() {
            if line.contains("{TAG}") {
                for tag in &nine_p_tags {
                    expanded.push_str(&line.replace("{TAG}", tag));
                    expanded.push('\n');
                }
            } else {
                expanded.push_str(line);
                expanded.push('\n');
            }
        }

        let header = if nine_p_tags.len() > 1 {
            format!("{} (repeat for each folder):", title)
        } else {
            format!("{}:", title)
        };
        blocks.push(format!("{}\n{}", header, expanded.trim_end()));
    }

    if let Some(folder) = smb_folder {
        let smb_key = format!("smb_{}", tier);
        let (title, description) = if app.shared_folders_help.has(&smb_key) {
            app.shared_folders_help.get_or_default(&smb_key)
        } else {
            app.shared_folders_help.get_or_default("smb")
        };
        let expanded = description
            .replace("{SHARE}", SMB_GUEST_SHARE)
            .replace("{PATH}", &folder.host_path);
        blocks.push(format!("{}:\n{}", title, expanded.trim_end()));
    }

    blocks.join("\n\n")
}

/// Determine the mount instruction tier based on the selected VM's OS profile
pub fn get_mount_tier(app: &App) -> &'static str {
    let vm = match app.selected_vm() {
        Some(vm) => vm,
        None => return "unknown",
    };

    // Get the OS profile ID from vm-curator.toml metadata, fall back to directory ID
    let profile_id = vm.os_profile.clone().or_else(|| Some(vm.id.clone()));

    // Check specific unsupported profile IDs first
    let unsupported_profiles = [
        "windows-95",
        "windows-98",
        "windows-me",
        "windows-2000",
        "windows-xp",
        "windows-vista",
        "windows-nt",
        "ms-dos",
        "freedos",
        "cpm",
    ];
    if let Some(ref id) = profile_id {
        if unsupported_profiles.iter().any(|p| id == p) {
            return "not_supported";
        }
    }

    // Check emulator for unsupported architectures
    let emulator = vm.config.emulator.command();
    if emulator.contains("m68k") || emulator.contains("ppc") {
        return "not_supported";
    }

    // Check specific supported Windows profiles
    if let Some(ref id) = profile_id {
        let windows_ids = [
            "windows-7",
            "windows-8",
            "windows-10",
            "windows-11",
            "generic-windows",
        ];
        if windows_ids.iter().any(|w| id == w) || id.starts_with("windows-server-") {
            return "windows";
        }
    }

    // Look up profile category
    if let Some(ref id) = profile_id {
        if let Some(profile) = app.qemu_profiles.get(id) {
            return match profile.category.as_str() {
                "linux" | "infrastructure" | "utilities" | "mobile" => "linux",
                "bsd" => "bsd",
                "macos" => "macos",
                "unix" => "unix",
                "alternative" => "alternative",
                "retro" | "classic-mac" => "not_supported",
                "windows" => "windows",
                _ => "unknown",
            };
        }
    }

    "unknown"
}

/// Handle key input for the shared folders screen
pub fn handle_key(app: &mut App, key: KeyEvent) -> Result<()> {
    match key.code {
        KeyCode::Esc => {
            if app.shared_folders_dirty() {
                app.push_screen(Screen::Confirm(ConfirmAction::UnsavedChanges(
                    UnsavedKind::SharedFolders,
                )));
            } else {
                app.pop_screen();
            }
        }
        KeyCode::Char('j') | KeyCode::Down => {
            if !app.shared_folders.is_empty()
                && app.shared_folder_selected < app.shared_folders.len() - 1
            {
                app.shared_folder_selected += 1;
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            if app.shared_folder_selected > 0 {
                app.shared_folder_selected -= 1;
            }
        }
        KeyCode::Char('a') | KeyCode::Char('A') => {
            app.load_file_browser(FileBrowserMode::Directory);
            app.push_screen(Screen::FileBrowser);
        }
        KeyCode::Char('d') | KeyCode::Char('D') => {
            app.remove_shared_folder();
        }
        KeyCode::Char('m') | KeyCode::Char('M') => {
            if app.shared_folders.is_empty() {
                app.set_status("No shared folder selected; press [a] to add one".to_string());
            } else {
                match app.toggle_shared_folder_mode() {
                    Ok(ShareMode::Smb) => app.set_status(format!(
                        "Folder set to SMB mode: guest sees it at {} (press [s] to save)",
                        SMB_GUEST_SHARE
                    )),
                    Ok(ShareMode::NineP) => {
                        app.set_status("Folder set to 9p mode (press [s] to save)".to_string())
                    }
                    Err(reason) => app.set_status(reason),
                }
            }
        }
        KeyCode::Char('s') | KeyCode::Char('S') => {
            save_selection_and_report(app);
        }
        _ => {}
    }
    Ok(())
}

/// Save shared folders to launch.sh and report the outcome via the status line.
/// Shared by the `s` key and the unsaved-changes prompt.
pub(crate) fn save_selection_and_report(app: &mut App) {
    let save_result = if let Some(vm) = app.selected_vm() {
        let result = crate::vm::save_shared_folders(vm, &app.shared_folders);
        Some((result, app.shared_folders.len()))
    } else {
        None
    };

    if let Some((result, count)) = save_result {
        match result {
            Ok(()) => {
                app.reload_selected_vm_script();

                let mut status_msg = if count > 0 {
                    format!("Saved {} shared folder(s) to launch.sh", count)
                } else {
                    "Cleared shared folders from launch.sh".to_string()
                };

                // QEMU spawns the host's smbd for SMB shares; warn now rather
                // than letting the VM fail to start later.
                let has_smb = app.shared_folders.iter().any(|f| f.mode == ShareMode::Smb);
                if has_smb && !crate::vm::smbd_available() {
                    status_msg.push_str(
                        "; warning: smbd not found on the host — install the samba package or QEMU will refuse to start the SMB share",
                    );
                }

                // Regenerate single-GPU scripts if they exist so their copied
                // QEMU command stays in sync with launch.sh.
                if let Some(vm) = app.selected_vm() {
                    let regen_result = if let Some(config) = app.single_gpu_config.as_ref() {
                        crate::vm::single_gpu_scripts::regenerate_if_exists(vm, config)
                    } else {
                        crate::vm::single_gpu_scripts::regenerate_from_saved_config(vm)
                    };

                    match regen_result {
                        Ok(true) => {
                            status_msg.push_str("; single-GPU scripts regenerated");
                        }
                        Ok(false) => {}
                        Err(e) => {
                            status_msg.push_str(&format!(
                                "; warning: failed to regenerate single-GPU scripts: {}",
                                e
                            ));
                        }
                    }
                }
                // Saved state is now the baseline; no unsaved changes remain.
                app.snapshot_shared_folders_baseline();
                app.set_status(status_msg);
            }
            Err(e) => {
                app.set_status(format!("Error saving shared folders: {}", e));
            }
        }
    }
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    Rect::new(x, y, width, height)
}
