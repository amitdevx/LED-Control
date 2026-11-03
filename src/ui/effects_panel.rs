// ponytail: Hardware modes and dynamic effects control panels for MELK-OA10.
// Separates hardware controller microchip routines from software dynamic effects.
// Strictly zero emojis and zero em-dashes.

use crate::effects::{Effect, EffectParams};
use gtk::prelude::*;
use gtk::{Box, Button, DropDown, Label, Orientation, Scale, StringList};
use std::cell::RefCell;
use std::rc::Rc;

pub struct HardwareModesHandle {
    #[allow(dead_code)]
    pub select_pattern: Rc<dyn Fn(u8)>,
}

pub struct DynamicEffectsHandle {
    #[allow(dead_code)]
    pub effect_dropdown: DropDown,
    #[allow(dead_code)]
    pub params: Rc<RefCell<EffectParams>>,
}

/// Panel for built-in hardware controller routines running directly on the MELK-OA10 chip.
pub fn create_hardware_modes_panel(
    on_pattern_select: impl Fn(u8) + Clone + 'static,
) -> (Box, HardwareModesHandle) {
    let container = Box::new(Orientation::Vertical, 8);
    container.add_css_class("effects-panel");

    let hw_label = Label::new(Some("Hardware Controller Modes (Verified for MELK-OA10)"));
    hw_label.add_css_class("section-label");
    hw_label.set_halign(gtk::Align::Start);
    container.append(&hw_label);

    let hw_desc = Label::new(Some(
        "Runs directly on the LED strip controller microchip (indices 0-210) with hardware PWM smoothing. \
         These routines execute autonomously inside the controller and continue even if the app closes.",
    ));
    hw_desc.add_css_class("dim-label");
    hw_desc.set_halign(gtk::Align::Start);
    hw_desc.set_wrap(true);
    container.append(&hw_desc);

    // Categories verified on MELK-OA10
    let pattern_categories: Vec<(&str, Vec<(u8, &str)>)> = vec![
        (
            "Hardware Fade & Color Flow",
            vec![
                (144, "7-Color Fade (Fade RGB)"),
                (16, "Rainbow Cycle"),
                (32, "Color Wave"),
                (48, "Breathing Flow"),
                (137, "RGB Crossfade"),
                (138, "7-Color Crossfade"),
            ],
        ),
        (
            "Hardware Jump & Snap",
            vec![
                (128, "RGB Jump"),
                (135, "3-Color Jump"),
                (136, "7-Color Jump"),
            ],
        ),
        (
            "Hardware Strobe & Blink",
            vec![
                (64, "Strobe Flash"),
                (149, "7-Color Blink"),
                (150, "Red Blink"),
                (151, "Green Blink"),
                (152, "Blue Blink"),
                (156, "White Blink"),
            ],
        ),
        (
            "Hardware Dynamic Modes",
            vec![
                (0, "Auto Play All"),
                (1, "Magic Flow"),
                (207, "Blue Scroll"),
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

    // Custom pattern ID row (0-210)
    let custom_row = Box::new(Orientation::Horizontal, 8);
    custom_row.set_margin_top(8);
    let custom_label = Label::new(Some("Direct routine ID (0-210):"));
    custom_label.add_css_class("dim-label");
    let custom_entry = gtk::Entry::new();
    custom_entry.set_placeholder_text(Some("0-210"));
    custom_entry.set_max_length(3);
    custom_entry.set_width_chars(6);
    let custom_btn = Button::with_label("Send ID");
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

    let all_btns_handle = pattern_buttons.clone();
    let select_pattern = Rc::new(move |target_idx: u8| {
        for (idx, b) in all_btns_handle.borrow().iter() {
            if *idx == target_idx {
                b.add_css_class("active");
            } else {
                b.remove_css_class("active");
            }
        }
    });

    let handle = HardwareModesHandle { select_pattern };
    (container, handle)
}

/// Panel for software dynamic effects generated live on the PC and streamed to the strip.
pub fn create_dynamic_effects_panel(
    on_effect_change: impl Fn(Effect) + Clone + 'static,
    on_params_change: impl Fn(&EffectParams) + Clone + 'static,
) -> (Box, DynamicEffectsHandle) {
    let container = Box::new(Orientation::Vertical, 8);
    container.add_css_class("effects-panel");

    let desc = Label::new(Some(
        "Software animation effects generated live and streamed at 30 Hz. \
         Smooth transitions and colors glide continuously without PWM jumps.",
    ));
    desc.add_css_class("dim-label");
    desc.set_halign(gtk::Align::Start);
    desc.set_wrap(true);
    container.append(&desc);

    let effect_label = Label::new(Some("Dynamic Effect Animation"));
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

    let params = Rc::new(RefCell::new(EffectParams::default()));

    // Speed slider
    let speed_row = create_slider_row("Animation Speed", 0.1, 5.0, 1.0, 0.1);
    let params_speed = params.clone();
    let on_params_speed = on_params_change.clone();
    speed_row.1.connect_value_changed(move |s| {
        params_speed.borrow_mut().speed = s.value();
        on_params_speed(&params_speed.borrow());
    });
    container.append(&speed_row.0);

    let handle = DynamicEffectsHandle {
        effect_dropdown,
        params,
    };
    (container, handle)
}

fn create_slider_row(label: &str, min: f64, max: f64, default: f64, step: f64) -> (Box, Scale) {
    let row = Box::new(Orientation::Horizontal, 8);

    let lbl = Label::new(Some(label));
    lbl.set_width_chars(15);
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
