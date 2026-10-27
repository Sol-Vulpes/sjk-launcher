//! The window: the two games with a big Play each, JoF content, and settings.

use crate::content::{COLLECTIONS, Collection};
use crate::state::{Core, Standing};
use crate::games::{GAMES, Game};
use crate::install::Phase;
use crate::system::{self, Tray, TrayAction};
use eframe::egui::{self, Color32, RichText, Vec2};
use sjk_jof_content::job::{Job, Phase as ContentPhase};
use sjk_jof_content::{AssetState, Plan, Selection, human_size};
use std::sync::atomic::Ordering;

const GOLD: Color32 = Color32::from_rgb(232, 184, 74);
const DIM: Color32 = Color32::from_rgb(150, 146, 160);
const BAD: Color32 = Color32::from_rgb(230, 110, 100);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Games,
    Content(Collection),
    Settings,
}

pub struct App {
    core: Core,
    tray: Option<Tray>,
    tab: Tab,
    /// The release picked in each game's list, by tag; `None` is the newest.
    picked: [Option<String>; 2],
    message: Option<(String, bool)>,
    /// One job and one choice per collection, in `COLLECTIONS`' order.
    content: [Job; 2],
    selections: [Selection; 2],
    tooltip: String,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, core: Core, minimized: bool) -> Self {
        style(&cc.egui_ctx);
        #[cfg(windows)]
        {
            use raw_window_handle::{HasWindowHandle, RawWindowHandle};
            if let Ok(handle) = cc.window_handle()
                && let RawWindowHandle::Win32(win32) = handle.as_raw()
            {
                core.hwnd.store(win32.hwnd.get(), Ordering::Relaxed);
            }
        }
        core.hidden.store(minimized, Ordering::Relaxed);
        let tray = make_tray(&core);
        let selections = COLLECTIONS.map(|collection| collection.selection(&core.config()));
        let mut app = Self {
            tray,
            tab: Tab::Games,
            picked: [None, None],
            message: None,
            content: Default::default(),
            selections,
            tooltip: String::new(),
            core,
        };
        for collection in COLLECTIONS {
            app.refresh_content(collection);
        }
        app
    }

    fn say(&mut self, text: impl Into<String>, bad: bool) {
        self.message = Some((text.into(), bad));
    }

    fn refresh_content(&mut self, collection: Collection) {
        if let Some(game_data) = self.core.game_data() {
            let settings = collection.settings(&self.core.config(), &game_data);
            self.content[collection as usize].refresh(settings);
        }
    }

    fn save_selection(&self, collection: Collection) {
        let selection = &self.selections[collection as usize];
        self.core.edit(|config| collection.save(config, selection));
    }

    fn hide(&self, ctx: &egui::Context) {
        self.core.hidden.store(true, Ordering::Relaxed);
        if cfg!(windows) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        } else {
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
        }
    }
}

fn make_tray(core: &Core) -> Option<Tray> {
    let handler = {
        let core = core.clone();
        move |action: TrayAction| match action {
            TrayAction::Show => {
                core.hidden.store(false, Ordering::Relaxed);
                system::show_window(core.hwnd.load(Ordering::Relaxed));
                core.ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                core.ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                core.ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                core.ctx.request_repaint();
            }
            TrayAction::Play(game) => {
                let _ = core.play(game);
            }
            TrayAction::Check => core.check_all(),
            TrayAction::Quit => std::process::exit(0),
        }
    };
    // Linux trays need GTK's loop on a thread of its own.
    #[cfg(target_os = "linux")]
    {
        std::thread::spawn(move || {
            if gtk::init().is_ok()
                && let Ok(_tray) = Tray::new(handler)
            {
                gtk::main();
            }
        });
        None
    }
    #[cfg(not(target_os = "linux"))]
    Tray::new(handler).ok()
}

fn style(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = Color32::from_rgb(20, 19, 26);
    visuals.window_fill = Color32::from_rgb(28, 26, 36);
    visuals.extreme_bg_color = Color32::from_rgb(14, 13, 18);
    visuals.selection.bg_fill = Color32::from_rgb(120, 92, 30);
    visuals.hyperlink_color = GOLD;
    ctx.set_visuals(visuals);
    ctx.style_mut(|style| {
        style.spacing.item_spacing = Vec2::new(8.0, 8.0);
        style.spacing.button_padding = Vec2::new(12.0, 6.0);
    });
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.core.hidden.load(Ordering::Relaxed) && !ctx.input(|input| input.viewport().minimized.unwrap_or(false)) {
            // Started with --minimized, or hidden from the tray before this frame.
            self.hide(ctx);
        }
        // Linux's tray lives on its own thread, so `self.tray` is empty there.
        let has_tray = self.tray.is_some() || cfg!(target_os = "linux");
        if ctx.input(|input| input.viewport().close_requested()) && self.core.config().close_to_tray && has_tray {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.hide(ctx);
        }
        let summary = self.core.summary();
        if summary != self.tooltip {
            if let Some(tray) = &self.tray {
                tray.set_tooltip(&summary);
            }
            self.tooltip = summary;
        }

        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("SJK Launcher").size(22.0).strong().color(GOLD));
                ui.add_space(16.0);
                let tabs = [(Tab::Games, "Games")]
                    .into_iter()
                    .chain(COLLECTIONS.map(|collection| (Tab::Content(collection), collection.name())))
                    .chain([(Tab::Settings, "Settings")]);
                for (tab, label) in tabs {
                    ui.selectable_value(&mut self.tab, tab, RichText::new(label).size(15.0));
                }
            });
            ui.add_space(4.0);
            self.game_data_row(ui);
            ui.add_space(6.0);
        });
        if let Some((text, bad)) = self.message.clone() {
            egui::TopBottomPanel::bottom("message").show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(text).color(if bad { BAD } else { DIM }));
                    if ui.small_button("✕").clicked() {
                        self.message = None;
                    }
                });
            });
        }
        egui::CentralPanel::default().show(ctx, |ui| match self.tab {
            Tab::Games => self.games(ui),
            Tab::Content(collection) => self.content_tab(ui, collection),
            Tab::Settings => self.settings(ui),
        });

        let busy = GAMES.iter().any(|&game| self.core.slot(game).install.running()) || self.content.iter().any(Job::busy);
        ctx.request_repaint_after(std::time::Duration::from_millis(if busy { 100 } else { 1000 }));
        #[cfg(debug_assertions)]
        debug_shot(ctx, &mut self.tab);
    }
}

/// Debug builds only: `SJK_LAUNCHER_SHOT=<file.bmp>` saves the window after a few
/// seconds, `SJK_LAUNCHER_TAB=content|settings` picks the tab first.
#[cfg(debug_assertions)]
fn debug_shot(ctx: &egui::Context, tab: &mut Tab) {
    let Ok(path) = std::env::var("SJK_LAUNCHER_SHOT") else { return };
    let wait: f64 = std::env::var("SJK_LAUNCHER_WAIT").ok().and_then(|w| w.parse().ok()).unwrap_or(6.0);
    let time = ctx.input(|input| input.time);
    if time < 1.0 {
        *tab = match std::env::var("SJK_LAUNCHER_TAB").as_deref() {
            Ok("content") => Tab::Content(Collection::Jof),
            Ok("sith") => Tab::Content(Collection::Sith),
            Ok("settings") => Tab::Settings,
            _ => Tab::Games,
        };
    }
    if time > wait && ctx.memory(|memory| memory.data.get_temp::<bool>(egui::Id::new("shot")).is_none()) {
        ctx.memory_mut(|memory| memory.data.insert_temp(egui::Id::new("shot"), true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
    }
    for event in ctx.input(|input| input.raw.events.clone()) {
        if let egui::Event::Screenshot { image, .. } = event {
            let [width, height] = image.size;
            let mut bmp = Vec::with_capacity(54 + width * height * 4);
            let size = (54 + width * height * 4) as u32;
            bmp.extend_from_slice(b"BM");
            bmp.extend_from_slice(&size.to_le_bytes());
            bmp.extend_from_slice(&[0, 0, 0, 0, 54, 0, 0, 0, 40, 0, 0, 0]);
            bmp.extend_from_slice(&(width as i32).to_le_bytes());
            bmp.extend_from_slice(&(-(height as i32)).to_le_bytes());
            bmp.extend_from_slice(&[1, 0, 32, 0]);
            bmp.extend_from_slice(&[0; 24]);
            for pixel in &image.pixels {
                bmp.extend_from_slice(&[pixel.b(), pixel.g(), pixel.r(), 255]);
            }
            let _ = std::fs::write(&path, bmp);
            std::process::exit(0);
        }
    }
    ctx.request_repaint();
}

impl App {
    fn game_data_row(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("GameData").color(DIM));
            match self.core.game_data() {
                Some(path) => ui.monospace(path.display().to_string()),
                None => ui.label(RichText::new("not found: choose Jedi Academy's GameData folder").color(BAD)),
            };
            if ui.button("Change…").clicked()
                && let Some(folder) = rfd::FileDialog::new().set_title("Jedi Academy's GameData folder").pick_folder()
            {
                let folder = crate::gamedata::normalise(&folder);
                if crate::gamedata::is_game_data(&folder) {
                    self.core.edit(|config| config.game_data = Some(folder));
                    for collection in COLLECTIONS {
                        self.refresh_content(collection);
                    }
                    self.say("GameData set", false);
                } else {
                    self.say(format!("{} has no base/assets0.pk3; pick Jedi Academy's GameData", folder.display()), true);
                }
            }
            if let Some(path) = self.core.game_data()
                && ui.button("Open folder").clicked()
            {
                ui.ctx().open_url(egui::OpenUrl::new_tab(format!("file:///{}", path.display())));
            }
        });
    }

    fn games(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.0);
        ui.columns(2, |columns| {
            for (index, game) in GAMES.into_iter().enumerate() {
                egui::Frame::group(columns[index].style())
                    .fill(Color32::from_rgb(28, 26, 36))
                    .inner_margin(16.0)
                    .corner_radius(10.0)
                    .show(&mut columns[index], |ui| {
                        ui.set_width(ui.available_width());
                        self.game_card(ui, game);
                    });
            }
        });
    }

    fn game_card(&mut self, ui: &mut egui::Ui, game: Game) {
        let core = self.core.clone();
        let slot = core.slot(game);
        ui.label(RichText::new(game.name()).size(24.0).strong());
        ui.label(RichText::new(game.blurb()).color(DIM));
        ui.add_space(6.0);

        let standing = core.standing(game);
        let (installed, colour) = match &standing {
            Standing::NotInstalled => ("Not installed".to_owned(), DIM),
            Standing::Unknown => ("Installed (version unknown)".to_owned(), DIM),
            Standing::UpToDate(tag) => (format!("{}  ✔ up to date", game.version_of(tag)), Color32::LIGHT_GREEN),
            Standing::Behind { installed, .. } => (game.version_of(installed), GOLD),
            Standing::Installed(tag) => (game.version_of(tag), Color32::WHITE),
        };
        egui::Grid::new(("versions", game as usize)).num_columns(2).spacing([12.0, 4.0]).show(ui, |ui| {
            ui.label(RichText::new("You play").color(DIM));
            ui.label(RichText::new(installed).color(colour).strong());
            ui.end_row();
            ui.label(RichText::new("Latest").color(DIM));
            match core.releases(game) {
                _ if slot.checking.load(Ordering::Relaxed) => ui.label("checking…"),
                None => ui.label("—"),
                Some(Err(error)) => ui.label(RichText::new(error).color(BAD)),
                Some(Ok(releases)) => match releases.first() {
                    Some(latest) => ui.label(format!("{}  ({})", game.version_of(&latest.tag_name), latest.date())),
                    None => ui.label("no build for this computer"),
                },
            };
            ui.end_row();
        });

        ui.add_space(6.0);
        let releases = core.releases(game).and_then(Result::ok).unwrap_or_default();
        let picked_tag = self.picked[game as usize].clone();
        let picked = picked_tag
            .as_ref()
            .and_then(|tag| releases.iter().find(|release| &release.tag_name == tag))
            .or(releases.first())
            .cloned();
        let phase = slot.install.phase();
        ui.horizontal(|ui| {
            let label = picked.as_ref().map_or("—".to_owned(), |release| {
                format!("{}  {}", game.version_of(&release.tag_name), release.date())
            });
            egui::ComboBox::from_id_salt(("pick", game as usize))
                .width(200.0)
                .selected_text(label)
                .show_ui(ui, |ui| {
                    for (index, release) in releases.iter().enumerate() {
                        let mut text = format!("{}  {}", game.version_of(&release.tag_name), release.date());
                        if index == 0 {
                            text.push_str("  (latest)");
                        }
                        if release.prerelease {
                            text.push_str("  pre-release");
                        }
                        let selected = picked.as_ref().is_some_and(|picked| picked.tag_name == release.tag_name);
                        if ui.selectable_label(selected, text).clicked() {
                            self.picked[game as usize] = (index != 0).then(|| release.tag_name.clone());
                        }
                    }
                });
            if let Some(release) = &picked {
                let installed_tag = core.config().installed.get(&game).cloned();
                let verb = match (&standing, installed_tag.as_deref() == Some(release.tag_name.as_str())) {
                    (Standing::NotInstalled, _) => "Install",
                    (_, true) => "Reinstall",
                    _ if releases.first().is_some_and(|latest| latest.tag_name == release.tag_name) => "Update",
                    _ => "Switch",
                };
                let enabled = !phase.running() && !core.running(game) && core.game_data().is_some();
                if ui.add_enabled(enabled, egui::Button::new(verb)).clicked() {
                    core.install(game, release.clone());
                }
                if ui.link("Notes").clicked() {
                    ui.ctx().open_url(egui::OpenUrl::new_tab(&release.html_url));
                }
            }
            if ui.small_button("⟳").on_hover_text("Check GitHub again").clicked() {
                core.check(game);
            }
        });

        match &phase {
            Phase::Idle => {}
            Phase::Downloading { done, total } => {
                let fraction = if *total > 0 { *done as f32 / *total as f32 } else { 0.0 };
                ui.horizontal(|ui| {
                    ui.add(
                        egui::ProgressBar::new(fraction)
                            .desired_width(ui.available_width() - 60.0)
                            .text(format!("Downloading {} of {}", human_size(*done), human_size(*total))),
                    );
                    if ui.button("Stop").clicked() {
                        slot.install.cancel();
                    }
                });
            }
            Phase::Checking => {
                ui.add(egui::ProgressBar::new(1.0).text("Checking the download"));
            }
            Phase::Extracting { done, total } => {
                ui.add(egui::ProgressBar::new(*done as f32 / (*total).max(1) as f32).text("Installing"));
            }
            Phase::Done(text) => {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(text).color(Color32::LIGHT_GREEN));
                    if ui.small_button("✕").clicked() {
                        slot.install.clear();
                    }
                });
            }
            Phase::Failed(text) => {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(text).color(BAD));
                    if ui.small_button("✕").clicked() {
                        slot.install.clear();
                    }
                });
            }
        }

        ui.add_space(12.0);
        let running = core.running(game);
        let present = core.game_data().is_some_and(|game_data| game.is_present(&game_data));
        let (text, enabled) = if running {
            ("RUNNING", false)
        } else if phase.running() {
            ("INSTALLING…", false)
        } else if !present {
            ("NOT INSTALLED", false)
        } else {
            ("PLAY", true)
        };
        let button = egui::Button::new(RichText::new(text).size(28.0).strong().color(Color32::BLACK))
            .fill(if enabled { GOLD } else { Color32::from_rgb(70, 64, 50) })
            .corner_radius(10.0)
            .min_size(Vec2::new(ui.available_width(), 72.0));
        // A justified layout centres the button's text.
        let clicked = ui
            .with_layout(egui::Layout::top_down_justified(egui::Align::Center), |ui| {
                ui.add_enabled(enabled, button).clicked()
            })
            .inner;
        if clicked
            && let Err(error) = core.play(game)
        {
            self.say(error, true);
        }
    }

    fn content_tab(&mut self, ui: &mut egui::Ui, collection: Collection) {
        let index = collection as usize;
        ui.add_space(6.0);
        ui.label(RichText::new(collection.about()).color(DIM));
        if self.core.game_data().is_none() {
            ui.label(RichText::new("Choose the GameData folder first.").color(BAD));
            return;
        }
        let progress = self.content[index].progress();
        let busy = self.content[index].busy();
        let plan = progress.snapshot.as_ref().map(|snapshot| Plan::new(snapshot, &self.selections[index]));

        ui.horizontal(|ui| {
            match &progress.phase {
                ContentPhase::Idle => {}
                ContentPhase::Manifest => _ = ui.label("Asking for the list…"),
                ContentPhase::Checking { done, total } => {
                    ui.add(
                        egui::ProgressBar::new(*done as f32 / (*total).max(1) as f32)
                            .desired_width(300.0)
                            .text("Checking files already here"),
                    );
                }
                ContentPhase::Removing { done, total } => _ = ui.label(format!("Removing {done} of {total}")),
                ContentPhase::Downloading { file, files, name, done, total, .. } => {
                    ui.add(
                        egui::ProgressBar::new(*done as f32 / (*total).max(1) as f32)
                            .desired_width(420.0)
                            .text(format!("{file} of {files}: {name}")),
                    );
                }
                ContentPhase::Finished(outcome) => {
                    let mut text = if outcome.synced {
                        format!("Done: {} downloaded ({}), {} removed", outcome.downloaded, human_size(outcome.downloaded_bytes), outcome.removed)
                    } else {
                        format!("List from {}", outcome.source)
                    };
                    if !outcome.failures.is_empty() {
                        text.push_str(&format!(", {} failed: {}", outcome.failures.len(), outcome.failures.join("; ")));
                    }
                    ui.label(RichText::new(text).color(if outcome.failures.is_empty() { DIM } else { BAD }));
                }
                ContentPhase::Failed(reason) => _ = ui.label(RichText::new(reason).color(BAD)),
                ContentPhase::Cancelled => _ = ui.label("Stopped; it carries on from where it was next time."),
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if busy {
                    if ui.button("Stop").clicked() {
                        self.content[index].cancel();
                    }
                } else if let Some(plan) = &plan {
                    let label = if !plan.downloads.is_empty() {
                        format!("Download {}", human_size(plan.download_bytes))
                    } else if !plan.removals.is_empty() {
                        format!("Remove {}", human_size(plan.removal_bytes))
                    } else {
                        "Up to date".to_owned()
                    };
                    let button = egui::Button::new(RichText::new(label).strong().color(Color32::BLACK)).fill(GOLD);
                    if ui.add_enabled(!plan.is_empty(), button).clicked()
                        && let Some(game_data) = self.core.game_data()
                    {
                        let settings = collection.settings(&self.core.config(), &game_data);
                        self.content[index].sync(settings, self.selections[index].clone());
                    }
                }
                if ui.add_enabled(!busy, egui::Button::new("Check again")).clicked() {
                    self.refresh_content(collection);
                }
            });
        });
        ui.separator();

        let Some(snapshot) = progress.snapshot else { return };
        let mut changed = false;
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            for (group_index, group) in snapshot.manifest.groups.iter().enumerate() {
                let total = group.assets.len();
                let here = (0..total).filter(|&asset| snapshot.state(group_index, asset).present()).count();
                let wanted = self.selections[index].wanted_in(group);
                let fetchable = group.assets.iter().any(|asset| asset.target == sjk_jof_content::Target::Base);
                ui.horizontal(|ui| {
                    let mut chosen = self.selections[index].chosen(group);
                    if ui.add_enabled(fetchable, egui::Checkbox::without_text(&mut chosen)).changed() {
                        self.selections[index].toggle(&snapshot.manifest, group.id);
                        changed = true;
                    }
                    ui.label(RichText::new(&group.name).strong().size(15.0));
                    ui.label(RichText::new(group.kind.label()).color(DIM));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new(format!("{here} of {total} here · {wanted} chosen · {}", human_size(group.assets.iter().map(|a| a.size).sum()))).color(DIM));
                    });
                });
                if !group.description.is_empty() {
                    ui.label(RichText::new(&group.description).color(DIM));
                }
                egui::CollapsingHeader::new("Files")
                    .id_salt(("files", group.id))
                    .show(ui, |ui| {
                        for (asset_index, asset) in group.assets.iter().enumerate() {
                            let state = snapshot.state(group_index, asset_index);
                            ui.horizontal(|ui| {
                                let mut wants = self.selections[index].wants(group, asset);
                                let enabled = state != AssetState::NotForSjk;
                                if ui.add_enabled(enabled, egui::Checkbox::without_text(&mut wants)).changed() {
                                    self.selections[index].toggle_file(group, asset);
                                    changed = true;
                                }
                                ui.label(&asset.name).on_hover_text(&asset.description);
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.label(RichText::new(human_size(asset.size)).color(DIM));
                                    let (text, colour) = match state {
                                        AssetState::Installed => ("Here", Color32::LIGHT_GREEN),
                                        AssetState::InGameFolder => ("In game folder", Color32::LIGHT_GREEN),
                                        AssetState::Outdated => ("To update", GOLD),
                                        AssetState::Missing if wants => ("To download", GOLD),
                                        AssetState::Missing => ("Not chosen", DIM),
                                        AssetState::NotForSjk => ("JoF's client only", DIM),
                                    };
                                    ui.label(RichText::new(text).color(colour));
                                });
                            });
                        }
                    });
                ui.separator();
            }
        });
        if changed {
            self.save_selection(collection);
        }
    }

    fn settings(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.0);
        let mut config = self.core.config().clone();
        let before = config.clone();

        ui.heading("Launcher");
        if ui.checkbox(&mut config.auto_start, "Start with my computer, in the tray").changed()
            && let Err(error) = system::set_autostart(config.auto_start)
        {
            self.say(format!("Start with the computer: {error}"), true);
            config.auto_start = !config.auto_start;
        }
        ui.checkbox(&mut config.close_to_tray, "Closing the window keeps the launcher in the tray");
        ui.checkbox(&mut config.auto_update, "Install new versions by themselves (when the game is closed)");
        ui.horizontal(|ui| {
            ui.label("Check for new versions every");
            ui.add(egui::DragValue::new(&mut config.check_hours).range(1..=48).suffix(" h"));
        });

        ui.add_space(8.0);
        ui.heading("Versions");
        for game in GAMES {
            let mut on = config.prereleases(game);
            if ui.checkbox(&mut on, format!("Offer {} pre-releases", game.name())).changed() {
                config.prereleases.insert(game, on);
            }
        }

        ui.add_space(8.0);
        ui.heading("Sith cloud");
        ui.horizontal(|ui| {
            ui.label("Server");
            ui.add(egui::TextEdit::singleline(&mut config.sith_url).desired_width(320.0));
            if ui.small_button("Default").clicked() {
                config.sith_url = crate::config::SITH_URL.into();
            }
        });

        ui.add_space(8.0);
        if let Some(folder) = crate::config::folder() {
            ui.label(RichText::new(format!("Settings: {}", folder.join("launcher.json").display())).color(DIM));
        }

        if format!("{config:?}") != format!("{before:?}") {
            let prereleases_changed = config.prereleases != before.prereleases;
            self.core.edit(|saved| *saved = config);
            if prereleases_changed {
                self.core.check_all();
            }
        }
    }
}
