//! Starting with the system, and the tray icon with its menu.

use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

pub const MINIMIZED: &str = "--minimized";

fn autostart() -> Result<auto_launch::AutoLaunch, String> {
    let program = std::env::current_exe().map_err(|error| error.to_string())?;
    auto_launch::AutoLaunchBuilder::new()
        .set_app_name("SJK Launcher")
        .set_app_path(&program.to_string_lossy())
        .set_args(&[MINIMIZED])
        .set_use_launch_agent(true)
        .build()
        .map_err(|error| error.to_string())
}

pub fn set_autostart(on: bool) -> Result<(), String> {
    let launch = autostart()?;
    let result = if on { launch.enable() } else { launch.disable() };
    result.map_err(|error| error.to_string())
}

/// The launcher's icon: a gold saber hilt on a dark disc, drawn here so no image ships.
pub fn icon_rgba(size: u32) -> Vec<u8> {
    let mut pixels = vec![0_u8; (size * size * 4) as usize];
    let centre = size as f32 / 2.0;
    for y in 0..size {
        for x in 0..size {
            let (fx, fy) = (x as f32 + 0.5 - centre, y as f32 + 0.5 - centre);
            let radius = (fx * fx + fy * fy).sqrt() / centre;
            // The blade runs corner to corner.
            let along = (fx - fy) / std::f32::consts::SQRT_2;
            let across = ((fx + fy) / std::f32::consts::SQRT_2).abs() / centre;
            let index = ((y * size + x) * 4) as usize;
            let colour: [u8; 4] = if radius > 1.0 {
                [0, 0, 0, 0]
            } else if across < 0.09 && along / centre > -0.15 && along / centre < 0.75 {
                [255, 214, 120, 255]
            } else if across < 0.14 && along / centre <= -0.15 && along / centre > -0.6 {
                [150, 150, 160, 255]
            } else if across < 0.2 && along / centre > 0.6 {
                [255, 196, 64, 90]
            } else {
                [24, 22, 30, 255]
            };
            pixels[index..index + 4].copy_from_slice(&colour);
        }
    }
    pixels
}

pub struct Tray {
    icon: TrayIcon,
}

/// What a tray click or menu item asks the launcher to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayAction {
    Show,
    Play(crate::games::Game),
    Check,
    Quit,
}

impl Tray {
    /// `on_action` runs on whichever thread delivers tray events; it must not block.
    pub fn new(on_action: impl Fn(TrayAction) + Send + Sync + 'static) -> Result<Self, String> {
        use crate::games::Game;
        let menu = Menu::new();
        let open = MenuItem::new("Open launcher", true, None);
        let play_sjk = MenuItem::new("Play Sol JK", true, None);
        let play_jof = MenuItem::new("Play JoF EternalJK", true, None);
        let check = MenuItem::new("Check for updates", true, None);
        let quit = MenuItem::new("Quit", true, None);
        menu.append_items(&[
            &open,
            &PredefinedMenuItem::separator(),
            &play_sjk,
            &play_jof,
            &check,
            &PredefinedMenuItem::separator(),
            &quit,
        ])
        .map_err(|error| error.to_string())?;
        let icon = Icon::from_rgba(icon_rgba(32), 32, 32).map_err(|error| error.to_string())?;
        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("SJK Launcher")
            .with_icon(icon)
            .with_menu_on_left_click(false)
            .build()
            .map_err(|error| error.to_string())?;
        let actions: Vec<(MenuId, TrayAction)> = vec![
            (open.id().clone(), TrayAction::Show),
            (play_sjk.id().clone(), TrayAction::Play(Game::Sjk)),
            (play_jof.id().clone(), TrayAction::Play(Game::Jof)),
            (check.id().clone(), TrayAction::Check),
            (quit.id().clone(), TrayAction::Quit),
        ];
        let on_action = std::sync::Arc::new(on_action);
        let menu_action = std::sync::Arc::clone(&on_action);
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            if let Some((_, action)) = actions.iter().find(|(id, _)| *id == event.id) {
                menu_action(*action);
            }
        }));
        TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                on_action(TrayAction::Show);
            }
        }));
        Ok(Self { icon: tray })
    }

    pub fn set_tooltip(&self, text: &str) {
        let _ = self.icon.set_tooltip(Some(text));
    }
}
/// Show the hidden window. A hidden eframe window gets no frames on Windows, so
/// it is shown straight through the system rather than by a viewport command.
#[cfg(windows)]
pub fn show_window(hwnd: isize) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{SW_RESTORE, SW_SHOW, SetForegroundWindow, ShowWindow};
    if hwnd != 0 {
        unsafe {
            ShowWindow(hwnd as _, SW_SHOW);
            ShowWindow(hwnd as _, SW_RESTORE);
            SetForegroundWindow(hwnd as _);
        }
    }
}

#[cfg(not(windows))]
pub fn show_window(_hwnd: isize) {}
