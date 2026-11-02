// ponytail: Effects control panel for software animations and hardware controller patterns.
// Supports speed, brightness, category grid, and active state tracking.

use crate::effects::{Effect, EffectParams};
use gtk::prelude::*;
use gtk::{Box, Button, DropDown, Label, Orientation, Scale, StringList};
use std::cell::RefCell;
use std::rc::Rc;

/// Build the effects control panel.
pub fn create_effects_panel(
    on_effect_change: impl Fn(Effect) + Clone + 'static,
    on_params_change: impl Fn(&EffectParams) + Clone + 'static,
    on_brightness_change: impl Fn(u8) + Clone + 'static,
    on_pattern_select: impl Fn(u8) + Clone + 'static,
) -> (Box, EffectsPanelHandle) {
    let container = Box::new(Orientation::Vertical, 8);
    container.add_css_class("effects-panel");

    let params = Rc::new(RefCell::new(EffectParams::default()));

    // Effect selector
    let effect_label = Label::new(Some("Software Effect Mode"));
    effect_label.add_css_class("section-label");
    effect_label.set_halign(gtk::Align::Start);
    container.append(&effect_label);

    let effect_names: Vec<&str> = Effect::all_software().iter().map(|e| e.label()).collect();
    let string_list = StringList::new(&effect_names);
    let effect_dropdown = DropDown::new(Some(string_list), gtk::Expression::NONE);
    effect_dropdown.add_css_class("effect-dropdown");

    let on_effect_ref = on_effect_change.clone();
    effect_dropdown.connect_selected_notify(move |dd| {
        let idx = dd.selected() as usize;
        let effects = Effect::all_software();
        if let Some(effect) = effects.get(idx) {
            on_effect_ref(*effect);
        }
    });
    container.append(&effect_dropdown);

    // Speed slider
    let speed_row = create_slider_row("Speed", 0.1, 5.0, 1.0, 0.1);
    let params_speed = params.clone();
    let on_params_speed = on_params_change.clone();
    speed_row.1.connect_value_changed(move |s| {
        params_speed.borrow_mut().speed = s.value();
        on_params_speed(&params_speed.borrow());
    });
    container.append(&speed_row.0);

    // Master Brightness slider
    let brightness_label = Label::new(Some("Master Brightness"));
    brightness_label.add_css_class("section-label");
    brightness_label.set_halign(gtk::Align::Start);
    brightness_label.set_margin_top(8);
    container.append(&brightness_label);

    let brightness_scale = Scale::with_range(Orientation::Horizontal, 1.0, 100.0, 1.0);
    brightness_scale.set_value(100.0);
    brightness_scale.set_hexpand(true);
    brightness_scale.add_css_class("brightness-slider");

    let brightness_value = Label::new(Some("100%"));
    brightness_value.set_width_chars(5);
    brightness_value.add_css_class("value-label");

    let bv_ref = brightness_value.clone();
    let params_br = params.clone();
    let on_params_br = on_params_change.clone();
    let on_br = on_brightness_change.clone();
    brightness_scale.connect_value_changed(move |s| {
        let pct = s.value().round() as u8;
        bv_ref.set_text(&format!("{}%", pct));
        params_br.borrow_mut().brightness = pct as f64 / 100.0;
        on_params_br(&params_br.borrow());
        on_br(pct);
    });

    let brightness_row = Box::new(Orientation::Horizontal, 8);
    brightness_row.append(&brightness_scale);
    brightness_row.append(&brightness_value);
    container.append(&brightness_row);

    // Hardware patterns section
    let hw_label = Label::new(Some("Hardware Controller Patterns (Unverified)"));
    hw_label.add_css_class("section-label");
    hw_label.set_halign(gtk::Align::Start);
    hw_label.set_margin_top(12);
    container.append(&hw_label);

    let hw_desc = Label::new(Some(
        "Runs on the physical LED strip controller microchip (indices 0-210). \
         Pattern names are approximate reference mappings pending physical verification. \
         Continues running even after closing the desktop app.",
    ));
    hw_desc.add_css_class("dim-label");
    hw_desc.set_halign(gtk::Align::Start);
    hw_desc.set_wrap(true);
    container.append(&hw_desc);

    // Pattern grid with named categories
    let pattern_categories: Vec<(&str, Vec<(u8, &str)>)> = vec![
        (
            "Fade & Breathe",
            vec![
                (1, "Pattern 1 (unverified Red Fade)"),
                (2, "Pattern 2 (unverified Green Fade)"),
                (3, "Pattern 3 (unverified Blue Fade)"),
                (4, "Pattern 4 (unverified Yellow Fade)"),
                (5, "Pattern 5 (unverified Cyan Fade)"),
                (6, "Pattern 6 (unverified Purple Fade)"),
                (7, "Pattern 7 (unverified White Fade)"),
                (8, "Pattern 8 (unverified 7-Color Fade)"),
            ],
        ),
        (
            "Jump & Snap",
            vec![
                (9, "Pattern 9 (unverified 3-Color Jump)"),
                (10, "Pattern 10 (unverified 7-Color Jump)"),
                (11, "Pattern 11 (unverified RGB Jump)"),
            ],
        ),
        (
            "Strobe Flash",
            vec![
                (12, "Pattern 12 (unverified Red Strobe)"),
                (13, "Pattern 13 (unverified Green Strobe)"),
                (14, "Pattern 14 (unverified Blue Strobe)"),
                (15, "Pattern 15 (unverified Yellow Strobe)"),
                (16, "Pattern 16 (unverified Cyan Strobe)"),
                (17, "Pattern 17 (unverified Magenta Strobe)"),
                (18, "Pattern 18 (unverified White Strobe)"),
            ],
        ),
        (
            "Chase & Stream",
            vec![
                (20, "Pattern 20 (unverified Rainbow Chase)"),
                (25, "Pattern 25 (unverified Pastel Stream)"),
                (30, "Pattern 30 (unverified Fire Chase)"),
                (40, "Pattern 40 (unverified Cyan Meteor)"),
                (45, "Pattern 45 (unverified Comet Flow)"),
            ],
        ),
    ];

    let pattern_buttons: Rc<RefCell<Vec<(u8, Button)>>> = Rc::new(RefCell::new(Vec::new()));

    for (cat_name, patterns) in &pattern_categories {
        let cat_label = Label::new(Some(cat_name));
        cat_label.add_css_class("pattern-category");
        cat_label.set_halign(gtk::Align::Start);
        cat_label.set_margin_top(6);
        container.append(&cat_label);

        let flow = gtk::FlowBox::new();
        flow.set_max_children_per_line(4);
        flow.set_min_children_per_line(2);
        flow.set_selection_mode(gtk::SelectionMode::None);
        flow.set_homogeneous(true);
        flow.set_column_spacing(6);
        flow.set_row_spacing(6);

        for (idx, name) in patterns {
            let btn = Button::with_label(name);
            btn.add_css_class("pattern-btn");
            let pattern_idx = *idx;
            let on_pat = on_pattern_select.clone();
            let all_btns = pattern_buttons.clone();
            let btn_clone = btn.clone();

            btn.connect_clicked(move |_| {
                // Clear active from all buttons
                for (_, b) in all_btns.borrow().iter() {
                    b.remove_css_class("active");
                }
                btn_clone.add_css_class("active");
                on_pat(pattern_idx);
            });

            pattern_buttons.borrow_mut().push((*idx, btn.clone()));
            flow.insert(&btn, -1);
        }
        container.append(&flow);
    }

    // Custom pattern index entry
    let custom_row = Box::new(Orientation::Horizontal, 8);
    custom_row.set_margin_top(8);
    let custom_label = Label::new(Some("Custom controller pattern ID (0-210):"));
    custom_label.add_css_class("dim-label");
    let custom_entry = gtk::Entry::new();
    custom_entry.set_placeholder_text(Some("0-210"));
    custom_entry.set_max_length(3);
    custom_entry.set_width_chars(6);
    let custom_btn = Button::with_label("Send to Strip");
    custom_btn.add_css_class("suggested-action");

    let entry_ref = custom_entry.clone();
    let on_pat_custom = on_pattern_select.clone();
    let all_btns_custom = pattern_buttons.clone();
    custom_btn.connect_clicked(move |_| {
        let text = entry_ref.text().to_string();
        if let Ok(idx) = text.parse::<u8>() {
            if idx <= 210 {
                for (btn_idx, b) in all_btns_custom.borrow().iter() {
                    if *btn_idx == idx {
                        b.add_css_class("active");
                    } else {
                        b.remove_css_class("active");
                    }
                }
                on_pat_custom(idx);
            }
        }
    });

    custom_row.append(&custom_label);
    custom_row.append(&custom_entry);
    custom_row.append(&custom_btn);
    container.append(&custom_row);

    let br_scale_clone = brightness_scale.clone();
    let br_val_clone = brightness_value.clone();
    let set_brightness = Rc::new(move |pct: u8| {
        br_scale_clone.set_value(pct as f64);
        br_val_clone.set_text(&format!("{}%", pct));
    });

    let handle = EffectsPanelHandle {
        effect_dropdown,
        brightness_scale,
        params,
        set_brightness,
    };

    (container, handle)
}

/// Handle for programmatically updating the effects panel.
#[allow(dead_code)]
pub struct EffectsPanelHandle {
    pub effect_dropdown: DropDown,
    pub brightness_scale: Scale,
    pub params: Rc<RefCell<EffectParams>>,
    pub set_brightness: Rc<dyn Fn(u8)>,
}

fn create_slider_row(label: &str, min: f64, max: f64, default: f64, step: f64) -> (Box, Scale) {
    let row = Box::new(Orientation::Horizontal, 8);

    let lbl = Label::new(Some(label));
    lbl.set_width_chars(8);
    lbl.add_css_class("section-label");

    let scale = Scale::with_range(Orientation::Horizontal, min, max, step);
    scale.set_value(default);
    scale.set_hexpand(true);
    scale.set_draw_value(false);

    let value_label = Label::new(Some(&format!("{:.1}x", default)));
    value_label.set_width_chars(5);
    value_label.add_css_class("value-label");

    let vl = value_label.clone();
    scale.connect_value_changed(move |s| {
        vl.set_text(&format!("{:.1}x", s.value()));
    });

    row.append(&lbl);
    row.append(&scale);
    row.append(&value_label);

    (row, scale)
}
