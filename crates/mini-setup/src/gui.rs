//! The window a person double-clicks.
//!
//! Deliberately plain: five pages, no animation, no branding beyond a name,
//! and every consequential rule delegated to [`crate::flow::Wizard`] (which
//! is unit-tested) or to `mini-windows-setup` (which is integration-tested).
//! This module draws and forwards; it decides nothing.
//!
//! The install itself runs on the UI thread. That is a considered choice
//! rather than an oversight: the whole operation is a few megabytes of local
//! file copying with no network and no waiting on anything, so the window is
//! unresponsive for a fraction of a second, and a background thread would
//! add a state machine --- and a way to click Install twice --- to save
//! nothing a user could perceive.

use crate::args::{Args, Mode};
use crate::flow::{Page, Wizard, DESTROY_CONFIRMATION};
use crate::{payload, run};
use eframe::egui;
use mini_windows_setup::container::Container;
use mini_windows_setup::{PackageManifest, PlanKind, Setup};
use std::path::PathBuf;

/// Run the wizard. Returns whatever `eframe` returns.
pub fn run_wizard(args: Args) -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Mininet Setup")
            .with_inner_size([760.0, 620.0])
            .with_min_inner_size([640.0, 520.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Mininet Setup",
        options,
        Box::new(move |_creation| Ok(Box::new(SetupApp::new(args)))),
    )
}

struct SetupApp {
    args: Args,
    wizard: Wizard,
    payload_bytes: Vec<u8>,
    payload_origin: String,
    manifest: Option<PackageManifest>,
    payload_error: Option<String>,
}

impl SetupApp {
    fn new(args: Args) -> Self {
        let setup = run::setup_for(&args);
        let status = setup
            .status()
            .unwrap_or_else(|_| mini_windows_setup::SetupStatus {
                install_root: setup.layout().root().to_path_buf(),
                active: None,
                previous: None,
                installed_versions: Vec::new(),
                launch_path: None,
                user_data_root: setup.user_data_root().to_path_buf(),
                user_data_present: false,
            });
        let wizard = Wizard::new(&status, args.options.clone());
        let (payload_bytes, payload_origin, manifest, payload_error) =
            match payload::locate(args.payload.as_deref()) {
                Ok(found) => {
                    let bytes = found.bytes.into_owned();
                    match Container::open(&bytes) {
                        Ok(container) => {
                            let manifest = container.manifest().clone();
                            (bytes, found.origin, Some(manifest), None)
                        }
                        Err(error) => (Vec::new(), found.origin, None, Some(error.to_string())),
                    }
                }
                Err(error) => (Vec::new(), String::new(), None, Some(error)),
            };
        Self {
            args,
            wizard,
            payload_bytes,
            payload_origin,
            manifest,
            payload_error,
        }
    }

    /// Arguments reflecting the wizard's current choices.
    fn effective_args(&self) -> Args {
        Args {
            mode: Mode::Silent,
            payload: self.args.payload.clone(),
            install_root: Some(PathBuf::from(self.wizard.install_root.trim())),
            user_data_root: self.args.user_data_root.clone(),
            options: self.wizard.options.clone(),
            destroy_identities: self.wizard.can_destroy_identities(),
            json: false,
        }
    }

    fn setup(&self) -> Setup {
        run::setup_for(&self.effective_args())
    }

    fn plan_kind(&self) -> String {
        let Some(manifest) = &self.manifest else {
            return "install".to_string();
        };
        match self.setup().plan(manifest, &self.wizard.options) {
            Ok(plan) => match plan.kind {
                PlanKind::FirstInstall => "first install".to_string(),
                PlanKind::Upgrade => "upgrade".to_string(),
                PlanKind::Reinstall => "reinstall".to_string(),
                PlanKind::Downgrade => "downgrade".to_string(),
            },
            Err(_) => "install".to_string(),
        }
    }

    /// Record the result of an action on the Result page.
    fn finish(&mut self, result: run::Result<run::Outcome>) {
        match result {
            Ok(outcome) => {
                self.wizard.succeed(outcome.human);
                self.wizard.launch_path = self
                    .setup()
                    .status()
                    .ok()
                    .and_then(|status| status.launch_path);
            }
            Err(failure) => self
                .wizard
                .fail(format!("{} [{}]", failure.message, failure.code)),
        }
    }

    /// Install exactly the bytes this window read and displayed, under the
    /// digest the user approved on the Review page.
    fn install_reviewed(&mut self) {
        let args = self.effective_args();
        let Some(manifest) = self.manifest.clone() else {
            self.wizard.fail("No package to install.");
            return;
        };
        let digest = manifest.digest_hex();
        let bytes = std::mem::take(&mut self.payload_bytes);
        let result = run::install_bytes(&args, &bytes, &self.payload_origin, Some(&digest));
        self.payload_bytes = bytes;
        self.finish(result);
    }

    fn welcome(&mut self, ui: &mut egui::Ui) {
        ui.heading("Install Mininet");
        ui.add_space(6.0);
        match (&self.manifest, &self.payload_error) {
            (Some(manifest), _) => {
                ui.label(format!("{} {}", manifest.product, manifest.version_text));
                ui.label(format!("Built for {}", manifest.target));
                ui.monospace(format!("Package digest {}", manifest.digest_hex()));
                ui.label(format!("Package source: {}", self.payload_origin));
                ui.add_space(8.0);
                ui.label(
                    "Setup installs into your own user profile. It needs no administrator \
                     rights, starts no service, and makes no network connection at any point.",
                );
                ui.label(
                    "Every file is checked against the package manifest as it is written and \
                     read back again before anything is activated.",
                );
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new(
                        "This build is not code-signed, so Windows SmartScreen will warn the \
                         first time you run it. You can check every file yourself with \
                         Get-FileHash against the manifest.",
                    )
                    .italics(),
                );
            }
            (None, Some(error)) => {
                ui.colored_label(egui::Color32::YELLOW, "No installable package was found.");
                ui.label(error);
                ui.label(format!(
                    "Put {} beside this program, or start it with --payload <FILE>.",
                    payload::SIDECAR_NAME
                ));
            }
            (None, None) => {
                ui.label("No package.");
            }
        }
    }

    fn options_page(&mut self, ui: &mut egui::Ui) {
        ui.heading("Where and how");
        ui.add_space(6.0);
        ui.label("Install location");
        let mut root = self.wizard.install_root.clone();
        if ui
            .add(egui::TextEdit::singleline(&mut root).desired_width(560.0))
            .changed()
        {
            self.wizard.set_install_root(root);
        }
        ui.label(
            egui::RichText::new(
                "Anywhere you can write. The default is inside your local app data, which is \
                 why no administrator prompt appears.",
            )
            .small(),
        );
        ui.add_space(10.0);
        let mut options = self.wizard.options.clone();
        let mut changed = false;
        changed |= ui
            .checkbox(&mut options.start_menu_shortcut, "Add a Start Menu entry")
            .changed();
        changed |= ui
            .checkbox(&mut options.desktop_shortcut, "Add a Desktop shortcut")
            .changed();
        changed |= ui
            .checkbox(
                &mut options.register_uninstall,
                "List in Apps & features, so it can be removed the usual way",
            )
            .changed();
        changed |= ui
            .checkbox(
                &mut options.allow_downgrade,
                "Allow installing an older version than the one already installed",
            )
            .changed();
        if changed {
            self.wizard.set_options(options);
        }
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new(
                "Identities, posts, and settings are kept separately and are never touched by \
                 installing, upgrading, or removing the program.",
            )
            .small(),
        );
    }

    fn review(&mut self, ui: &mut egui::Ui) {
        ui.heading("Review before installing");
        ui.add_space(6.0);
        let Some(manifest) = self.manifest.clone() else {
            ui.colored_label(egui::Color32::YELLOW, "No package to install.");
            return;
        };
        let kind = self.plan_kind();
        egui::ScrollArea::vertical()
            .max_height(330.0)
            .show(ui, |ui| {
                for line in self.wizard.review_lines(&manifest, &kind) {
                    ui.monospace(line);
                }
            });
        ui.add_space(8.0);
        let mut approved = self.wizard.approved;
        if ui
            .checkbox(
                &mut approved,
                format!(
                    "I approve installing this exact package ({}...)",
                    &manifest.digest_hex()[..16]
                ),
            )
            .changed()
        {
            self.wizard.approved = approved;
        }
        ui.label(
            egui::RichText::new(
                "Approval names the package digest above. If the package changes, the approval \
                 stops applying and setup will refuse to install it.",
            )
            .small(),
        );
    }

    fn maintenance(&mut self, ui: &mut egui::Ui) {
        ui.heading("Mininet is already installed");
        ui.add_space(6.0);
        let status = self.setup().status();
        match &status {
            Ok(status) => {
                if let Some(active) = &status.active {
                    ui.label(format!("Installed version: {}", active.version_text));
                    ui.monospace(format!("Package digest {}", active.package_digest));
                }
                if let Some(previous) = &status.previous {
                    ui.label(format!("Can roll back to {}", previous.version_text));
                }
                ui.label(format!("Program files: {}", status.install_root.display()));
                ui.label(format!("Your data: {}", status.user_data_root.display()));
            }
            Err(error) => {
                ui.colored_label(egui::Color32::YELLOW, error.to_string());
            }
        }
        ui.add_space(12.0);
        ui.horizontal_wrapped(|ui| {
            if ui.button("Check installed files").clicked() {
                // Explicitly without the payload: `run::verify` gives an
                // explicit payload precedence, so a wizard opened with
                // --payload while already installed would otherwise check the
                // package file and report success over a corrupted install.
                let mut args = self.effective_args();
                args.payload = None;
                let result = run::verify(&args);
                self.finish(result);
            }
            if let Some(manifest) = self.manifest.clone() {
                if ui
                    .button(format!("Install {} over it", manifest.version_text))
                    .clicked()
                {
                    self.wizard.page = Page::Options;
                }
            }
            let can_roll_back = status
                .as_ref()
                .map(|status| status.previous.is_some())
                .unwrap_or(false);
            if ui
                .add_enabled(can_roll_back, egui::Button::new("Roll back"))
                .clicked()
            {
                let result = run::rollback(&self.effective_args());
                self.finish(result);
            }
            if ui.button("Remove Mininet").clicked() {
                self.wizard.page = Page::ConfirmDestroy;
            }
        });
        ui.add_space(8.0);
        if let Ok(status) = &status {
            if let Some(path) = &status.launch_path {
                ui.label(format!("Start it from {}", path.display()));
            }
        }
    }

    fn confirm_remove(&mut self, ui: &mut egui::Ui) {
        ui.heading("Remove Mininet");
        ui.add_space(6.0);
        ui.label(
            "Removing the program deletes the installed files, the Start Menu entry, and the \
             Apps & features listing.",
        );
        ui.add_space(8.0);
        let user_data = self.setup().user_data_root().to_path_buf();
        ui.label(format!(
            "Your identities, posts, and settings in {} are kept unless you tick the box below.",
            user_data.display()
        ));
        ui.add_space(10.0);
        let mut destroy = self.wizard.destroy_identities;
        if ui
            .checkbox(
                &mut destroy,
                "Also delete my identities, posts, and settings",
            )
            .changed()
        {
            self.wizard.destroy_identities = destroy;
            if !destroy {
                self.wizard.destroy_confirmation.clear();
            }
        }
        if self.wizard.destroy_identities {
            ui.colored_label(
                egui::Color32::from_rgb(220, 120, 60),
                "This destroys the only copy of your signing keys. Nobody can restore them, and \
                 you cannot re-create the same identity. Everything you have signed stays \
                 published but you will never be able to sign as that identity again.",
            );
            ui.label(format!("Type {DESTROY_CONFIRMATION} to confirm:"));
            ui.add(
                egui::TextEdit::singleline(&mut self.wizard.destroy_confirmation)
                    .desired_width(200.0),
            );
        }
    }

    fn result(&mut self, ui: &mut egui::Ui) {
        if self.wizard.succeeded {
            ui.heading("Done");
        } else {
            ui.heading("That did not work");
        }
        ui.add_space(6.0);
        let colour = if self.wizard.succeeded {
            ui.visuals().text_color()
        } else {
            egui::Color32::from_rgb(220, 120, 60)
        };
        for line in self.wizard.message.lines() {
            ui.colored_label(colour, line);
        }
        ui.add_space(10.0);
        if self.wizard.succeeded {
            if let Some(path) = self.wizard.launch_path.clone() {
                if path.is_file() {
                    ui.label(format!("Installed client: {}", path.display()));
                }
            }
        }
    }

    fn footer(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        ui.horizontal(|ui| {
            let can_go_back = matches!(
                self.wizard.page,
                Page::Options | Page::Review | Page::Result | Page::ConfirmDestroy
            );
            if ui
                .add_enabled(can_go_back, egui::Button::new("Back"))
                .clicked()
            {
                self.wizard.back();
            }
            match self.wizard.page {
                Page::Welcome => {
                    let ready = self.manifest.is_some();
                    if ui.add_enabled(ready, egui::Button::new("Next")).clicked() {
                        self.wizard.next();
                    }
                }
                Page::Options => {
                    if ui.button("Next").clicked() {
                        self.wizard.next();
                    }
                }
                Page::Review => {
                    let can_install = self.wizard.can_install();
                    if ui
                        .add_enabled(can_install, egui::Button::new("Install"))
                        .clicked()
                    {
                        self.install_reviewed();
                    }
                    if !can_install {
                        ui.label(
                            egui::RichText::new("Tick the approval box to enable Install.").small(),
                        );
                    }
                }
                Page::ConfirmDestroy => {
                    let label = if self.wizard.destroy_identities {
                        "Remove and delete my data"
                    } else {
                        "Remove Mininet"
                    };
                    let ready =
                        !self.wizard.destroy_identities || self.wizard.can_destroy_identities();
                    if ui.add_enabled(ready, egui::Button::new(label)).clicked() {
                        let result = run::uninstall(&self.effective_args());
                        self.finish(result);
                    }
                }
                Page::Maintenance | Page::Result => {}
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new("No network. No administrator. No telemetry.").small(),
                );
            });
        });
    }
}

impl eframe::App for SetupApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::TopBottomPanel::bottom("footer").show_inside(ui, |ui| self.footer(ui));
            egui::ScrollArea::vertical().show(ui, |ui| match self.wizard.page {
                Page::Welcome => self.welcome(ui),
                Page::Options => self.options_page(ui),
                Page::Review => self.review(ui),
                Page::Maintenance => self.maintenance(ui),
                Page::ConfirmDestroy => self.confirm_remove(ui),
                Page::Result => self.result(ui),
            });
        });
    }
}
