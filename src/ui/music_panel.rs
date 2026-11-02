// Music sync control panel.
//
// ponytail: Provides interactive controls for audio reactivity and BPM tempo sync.

use crate::effects::{CaptureStatus, MusicEngine, MusicMode};
use gtk::prelude::*;
use gtk::{Box, CheckButton, DropDown, Label, Orientation, ProgressBar, Scale, StringList};
use std::cell::RefCell;
use std::rc::Rc;

pub fn create_music_panel(
    music_engine: Rc<RefCell<MusicEngine>>,
    on_active_toggle: impl Fn(bool) + Clone + 'static,
) -> Box {
    let container = Box::new(Orientation::Vertical, 10);
    container.add_css_class("effects-panel");

    let header_label = Label::new(Some("Music Sync & Audio Reactivity"));
    header_label.add_css_class("section-label");
    header_label.set_halign(gtk::Align::Start);
    container.append(&header_label);

    let desc_label = Label::new(Some(
        "Synchronize the LED strip to music rhythms using live desktop audio or internal beat tempo.",
    ));
    desc_label.add_css_class("dim-label");
    desc_label.set_halign(gtk::Align::Start);
    desc_label.set_wrap(true);
    container.append(&desc_label);

    // Active Toggle Switch
    let active_check = CheckButton::with_label("Enable Music Sync Mode");
    active_check.add_css_class("channel-label");
    active_check.set_margin_top(6);
    active_check.set_margin_bottom(6);
    let on_toggle = on_active_toggle.clone();
    active_check.connect_toggled(move |btn| {
        on_toggle(btn.is_active());
    });
    container.append(&active_check);

    // Mode Selector
    let mode_label = Label::new(Some("Rhythm Mode"));
    mode_label.add_css_class("section-label");
    mode_label.set_halign(gtk::Align::Start);
    container.append(&mode_label);

    let mode_names: Vec<&str> = MusicMode::all().iter().map(|m| m.label()).collect();
    let string_list = StringList::new(&mode_names);
    let mode_dropdown = DropDown::new(Some(string_list), gtk::Expression::NONE);
    mode_dropdown.add_css_class("effect-dropdown");

    let engine_mode = music_engine.clone();
    mode_dropdown.connect_selected_notify(move |dd| {
        let idx = dd.selected() as usize;
        let modes = MusicMode::all();
        if let Some(mode) = modes.get(idx) {
            engine_mode.borrow_mut().set_mode(*mode);
        }
    });
    container.append(&mode_dropdown);

    // Live Mic / Audio Monitor toggle
    let mic_check = CheckButton::with_label("Capture Live Audio (parec/PulseAudio)");
    mic_check.add_css_class("dim-label");
    mic_check.set_margin_top(8);

    let mic_status_label = Label::new(None);
    mic_status_label.add_css_class("dim-label");
    mic_status_label.set_halign(gtk::Align::Start);
    mic_status_label.set_visible(false);

    let is_internal_update = Rc::new(std::cell::Cell::new(false));
    let engine_mic = music_engine.clone();
    let status_lbl_clone = mic_status_label.clone();
    let is_internal_toggle = is_internal_update.clone();

    mic_check.connect_toggled(move |btn| {
        if is_internal_toggle.get() {
            return;
        }

        let active = btn.is_active();
        engine_mic.borrow_mut().set_use_live_mic(active);

        // ponytail: Scope the borrow to drop it before touching widget states
        let status = engine_mic.borrow().capture_status();

        match status {
            CaptureStatus::Active => {
                status_lbl_clone.set_text("Live audio capture active");
                status_lbl_clone.remove_css_class("error");
                status_lbl_clone.set_visible(true);
            }
            CaptureStatus::Failed(err) => {
                status_lbl_clone.set_text(&format!("Capture error: {}", err));
                status_lbl_clone.add_css_class("error");
                status_lbl_clone.set_visible(true);
                is_internal_toggle.set(true);
                btn.set_active(false);
                is_internal_toggle.set(false);
            }
            CaptureStatus::Inactive => {
                status_lbl_clone.set_visible(false);
            }
        }
    });
    container.append(&mic_check);
    container.append(&mic_status_label);

    // Sensitivity Slider
    let sens_label = Label::new(Some("Beat Sensitivity"));
    sens_label.add_css_class("section-label");
    sens_label.set_halign(gtk::Align::Start);
    sens_label.set_margin_top(8);
    container.append(&sens_label);

    let sens_row = Box::new(Orientation::Horizontal, 8);
    let sens_scale = Scale::with_range(Orientation::Horizontal, 0.2, 2.5, 0.1);
    sens_scale.set_value(1.0);
    sens_scale.set_hexpand(true);
    let sens_val_lbl = Label::new(Some("1.0x"));
    sens_val_lbl.set_width_chars(5);
    sens_val_lbl.add_css_class("value-label");

    let engine_sens = music_engine.clone();
    let sv_lbl = sens_val_lbl.clone();
    sens_scale.connect_value_changed(move |s| {
        let val = s.value();
        sv_lbl.set_text(&format!("{:.1}x", val));
        engine_sens.borrow_mut().set_sensitivity(val);
    });
    sens_row.append(&sens_scale);
    sens_row.append(&sens_val_lbl);
    container.append(&sens_row);

    // BPM Tempo Slider (for internal beat generator)
    let bpm_label = Label::new(Some("Tempo (BPM)"));
    bpm_label.add_css_class("section-label");
    bpm_label.set_halign(gtk::Align::Start);
    bpm_label.set_margin_top(8);
    container.append(&bpm_label);

    let bpm_row = Box::new(Orientation::Horizontal, 8);
    let bpm_scale = Scale::with_range(Orientation::Horizontal, 60.0, 180.0, 1.0);
    bpm_scale.set_value(128.0);
    bpm_scale.set_hexpand(true);
    let bpm_val_lbl = Label::new(Some("128 BPM"));
    bpm_val_lbl.set_width_chars(8);
    bpm_val_lbl.add_css_class("value-label");

    let engine_bpm = music_engine.clone();
    let bp_lbl = bpm_val_lbl.clone();
    bpm_scale.connect_value_changed(move |s| {
        let val = s.value();
        bp_lbl.set_text(&format!("{:.0} BPM", val));
        engine_bpm.borrow_mut().set_bpm(val);
    });
    bpm_row.append(&bpm_scale);
    bpm_row.append(&bpm_val_lbl);
    container.append(&bpm_row);

    // Live Audio VU Meter
    let vu_label = Label::new(Some("Audio Energy Level"));
    vu_label.add_css_class("section-label");
    vu_label.set_halign(gtk::Align::Start);
    vu_label.set_margin_top(8);
    container.append(&vu_label);

    let progress = ProgressBar::new();
    progress.set_fraction(0.0);
    progress.set_margin_bottom(12);
    container.append(&progress);

    // Update VU meter bar on tick using current_energy without double-ticking
    let prog_ref = progress.clone();
    let engine_vu = music_engine.clone();
    let status_lbl_timer = mic_status_label.clone();
    let mic_check_timer = mic_check.clone();
    let is_internal_timer = is_internal_update.clone();

    gtk::glib::timeout_add_local(std::time::Duration::from_millis(50), move || {
        // ponytail: Scope borrow to local variables so RefCell is not held during widget updates
        let (status, energy) = {
            let eng = engine_vu.borrow();
            (eng.capture_status(), eng.current_energy())
        };

        if mic_check_timer.is_active() {
            if let CaptureStatus::Failed(ref err) = status {
                status_lbl_timer.set_text(&format!("Capture error: {}", err));
                status_lbl_timer.add_css_class("error");
                status_lbl_timer.set_visible(true);
                is_internal_timer.set(true);
                mic_check_timer.set_active(false);
                is_internal_timer.set(false);
            }
        }
        prog_ref.set_fraction(energy);
        gtk::glib::ControlFlow::Continue
    });

    container
}
