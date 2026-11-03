// ponytail: Master brightness control panel with smooth scale and quick preset buttons.
// Configured specifically for MELK-OA10 LED strip (100%, 75%, 50%, 25%, 10%, 0%).
// Strictly zero emojis and zero em-dashes.

use gtk::prelude::*;
use gtk::{Box, Button, Label, Orientation, Scale};
use std::cell::RefCell;
use std::rc::Rc;

pub struct BrightnessHandle {
    #[allow(dead_code)]
    pub scale: Scale,
    #[allow(dead_code)]
    pub set_value: Rc<dyn Fn(u8)>,
}

pub fn create_brightness_control(
    initial_pct: u8,
    on_brightness_change: impl Fn(u8) + Clone + 'static,
) -> (Box, BrightnessHandle) {
    let container = Box::new(Orientation::Vertical, 6);
    container.add_css_class("brightness-card");

    // Header row: Label + Value
    let header_row = Box::new(Orientation::Horizontal, 8);
    let title_label = Label::new(Some("Master Brightness"));
    title_label.add_css_class("section-label");
    title_label.set_hexpand(true);
    title_label.set_halign(gtk::Align::Start);
    header_row.append(&title_label);

    let val_label = Label::new(Some(&format!("{initial_pct}%")));
    val_label.add_css_class("value-label");
    val_label.set_width_chars(5);
    header_row.append(&val_label);

    container.append(&header_row);

    // Presets row: 100%, 75%, 50%, 25%, 10%, 0% (Off)
    let presets_row = Box::new(Orientation::Horizontal, 6);
    presets_row.set_homogeneous(true);

    let presets: &[(u8, &str)] = &[
        (100, "100%"),
        (75, "75%"),
        (50, "50%"),
        (25, "25%"),
        (10, "10%"),
        (0, "0% (Off)"),
    ];

    let preset_buttons: Rc<RefCell<Vec<(u8, Button)>>> = Rc::new(RefCell::new(Vec::new()));
    let scale = Scale::with_range(Orientation::Horizontal, 0.0, 100.0, 1.0);
    scale.set_value(initial_pct as f64);
    scale.set_hexpand(true);
    scale.add_css_class("brightness-slider");

    for &(pct, label_text) in presets {
        let btn = Button::with_label(label_text);
        btn.add_css_class("brightness-preset-btn");
        if pct == initial_pct {
            btn.add_css_class("active");
        }
        let scale_ref = scale.clone();
        btn.connect_clicked(move |_| {
            scale_ref.set_value(pct as f64);
        });
        preset_buttons.borrow_mut().push((pct, btn.clone()));
        presets_row.append(&btn);
    }
    container.append(&presets_row);

    // Slider row
    let slider_row = Box::new(Orientation::Horizontal, 8);
    slider_row.append(&scale);
    container.append(&slider_row);

    // Helper to sync active preset button styling
    let sync_buttons = {
        let preset_buttons = preset_buttons.clone();
        Rc::new(move |cur_pct: u8| {
            for (p, b) in preset_buttons.borrow().iter() {
                if *p == cur_pct {
                    b.add_css_class("active");
                } else {
                    b.remove_css_class("active");
                }
            }
        })
    };

    let vl = val_label.clone();
    let on_change = on_brightness_change.clone();
    let sync_btn_clone = sync_buttons.clone();
    scale.connect_value_changed(move |s| {
        let pct = s.value().round() as u8;
        vl.set_text(&format!("{pct}%"));
        sync_btn_clone(pct);
        on_change(pct);
    });

    let scale_handle = scale.clone();
    let vl_handle = val_label.clone();
    let sync_btn_handle = sync_buttons.clone();
    let set_value = Rc::new(move |pct: u8| {
        scale_handle.set_value(pct as f64);
        vl_handle.set_text(&format!("{pct}%"));
        sync_btn_handle(pct);
    });

    let handle = BrightnessHandle { scale, set_value };

    (container, handle)
}
