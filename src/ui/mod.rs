// ponytail: UI module exports the individual panels and widget builders
pub mod brightness_panel;
pub mod color_picker;
pub mod connection_panel;
pub mod effects_panel;
pub mod music_panel;
pub mod settings_panel;
pub mod strip_widget;
pub mod styles;

pub use brightness_panel::create_brightness_control;
pub use color_picker::create_color_picker;
pub use connection_panel::create_connection_panel;
pub use effects_panel::{create_dynamic_effects_panel, create_hardware_modes_panel};
pub use music_panel::create_music_panel;
pub use settings_panel::create_settings_panel;
pub use strip_widget::{create_strip_widget, SMD_COUNT};
pub use styles::app_css;
