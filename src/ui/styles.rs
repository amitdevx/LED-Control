// ponytail: Application-wide CSS stylesheet with strict GTK4 compliance.
// Clean, modern dark theme without unsupported CSS properties or !important hacks.

/// Return the application CSS stylesheet as a string.
pub fn app_css() -> &'static str {
    r#"
    /* == Global Window & Typography == */
    window {
        background-color: #0c0e15;
        color: #e2e8f0;
        font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Cantarell", "Ubuntu", sans-serif;
    }

    /* == Header Bar == */
    headerbar {
        background-color: #111420;
        border-bottom: 1px solid rgba(255, 255, 255, 0.07);
        padding: 6px 12px;
        min-height: 48px;
    }

    headerbar .title {
        font-weight: 700;
        font-size: 14px;
        letter-spacing: 0.5px;
        color: #f8fafc;
    }

    headerbar .subtitle {
        font-size: 11px;
        color: #64748b;
    }

    /* == Main Container == */
    .main-container {
        margin: 14px 16px;
    }

    /* == Section Eyebrows & Labels == */
    .section-label {
        font-size: 11px;
        font-weight: 700;
        text-transform: uppercase;
        letter-spacing: 1.2px;
        color: #718096;
        margin-top: 6px;
        margin-bottom: 4px;
    }

    .dim-label {
        font-size: 12px;
        color: #64748b;
        line-height: 1.4;
    }

    /* == LED Strip Visualization Canvas == */
    .led-strip {
        background-color: #06070a;
        border-radius: 12px;
        border: 1px solid rgba(255, 255, 255, 0.06);
        box-shadow: 0 8px 30px rgba(0, 0, 0, 0.6);
        margin-bottom: 12px;
    }

    /* == Card Panels == */
    .color-picker-panel,
    .effects-panel,
    .connection-panel,
    .settings-panel {
        background-color: #141724;
        border: 1px solid rgba(255, 255, 255, 0.06);
        border-radius: 14px;
        padding: 16px;
        box-shadow: 0 4px 20px rgba(0, 0, 0, 0.25);
        margin-bottom: 8px;
    }

    /* == Global Brightness Card & Presets == */
    .brightness-card {
        background-color: #141724;
        border: 1px solid rgba(255, 255, 255, 0.06);
        border-radius: 12px;
        padding: 10px 16px;
        margin-bottom: 8px;
    }

    .brightness-preset-btn {
        font-family: monospace;
        font-size: 11px;
        font-weight: 700;
        padding: 6px 10px;
        border-radius: 6px;
        background-color: #1a1e30;
        color: #cbd5e1;
        border: 1px solid rgba(255, 255, 255, 0.08);
        transition: background-color 150ms ease, color 150ms ease, border-color 150ms ease;
    }

    .brightness-preset-btn:hover {
        background-color: #242b45;
        color: #ffffff;
        border-color: rgba(56, 189, 248, 0.4);
    }

    .brightness-preset-btn.active {
        background-color: #2563eb;
        color: #ffffff;
        border-color: #38bdf8;
    }

    /* == Notebook Tabs Navigation == */
    notebook {
        background-color: transparent;
    }

    notebook > header {
        background-color: #111420;
        border-radius: 10px;
        padding: 4px;
        border: 1px solid rgba(255, 255, 255, 0.06);
        margin-bottom: 10px;
    }

    notebook > header.top > tabs > tab {
        background-color: transparent;
        color: #94a3b8;
        font-size: 12px;
        font-weight: 600;
        padding: 7px 16px;
        border-radius: 8px;
        border: none;
        transition: background-color 150ms ease, color 150ms ease;
    }

    notebook > header.top > tabs > tab:checked {
        background-color: #1e2438;
        color: #38bdf8;
        box-shadow: 0 2px 8px rgba(0, 0, 0, 0.3);
    }

    notebook > header.top > tabs > tab:hover:not(:checked) {
        color: #f1f5f9;
        background-color: rgba(255, 255, 255, 0.04);
    }

    /* == Channel Sliders == */
    .channel-label {
        font-family: monospace;
        font-weight: 700;
        font-size: 13px;
        color: #cbd5e1;
    }

    .value-label {
        font-family: monospace;
        font-size: 12px;
        font-weight: 600;
        color: #94a3b8;
        min-width: 38px;
    }

    scale trough {
        background-color: #0b0d14;
        border-radius: 6px;
        min-height: 8px;
        border: 1px solid rgba(255, 255, 255, 0.05);
    }

    scale slider {
        background-color: #f8fafc;
        border-radius: 50%;
        min-width: 18px;
        min-height: 18px;
        border: 2px solid #0f172a;
        box-shadow: 0 2px 6px rgba(0, 0, 0, 0.4);
    }

    scale slider:hover {
        background-color: #ffffff;
    }

    .slider-red trough highlight {
        background-color: #ff2d55;
        border-radius: 6px;
    }

    .slider-green trough highlight {
        background-color: #00e676;
        border-radius: 6px;
    }

    .slider-blue trough highlight {
        background-color: #2979ff;
        border-radius: 6px;
    }

    .brightness-slider trough highlight {
        background-color: #e2e8f0;
        border-radius: 6px;
    }

    /* == Hex Input == */
    .hex-entry {
        font-family: monospace;
        font-size: 13px;
        font-weight: 600;
        background-color: #0b0d14;
        color: #f1f5f9;
        border: 1px solid rgba(255, 255, 255, 0.1);
        border-radius: 8px;
        padding: 6px 10px;
    }

    .hex-entry:focus {
        border-color: #38bdf8;
        box-shadow: 0 0 0 2px rgba(56, 189, 248, 0.2);
    }

    /* == Hardware Pattern Buttons == */
    .pattern-category {
        font-size: 11px;
        font-weight: 700;
        color: #38bdf8;
        letter-spacing: 0.5px;
        margin-top: 10px;
        margin-bottom: 4px;
    }

    .pattern-btn {
        font-size: 12px;
        font-weight: 500;
        padding: 6px 12px;
        border-radius: 8px;
        background-color: #1a1e30;
        color: #cbd5e1;
        border: 1px solid rgba(255, 255, 255, 0.08);
        transition: background-color 150ms ease, color 150ms ease, border-color 150ms ease;
    }

    .pattern-btn:hover {
        background-color: #242b45;
        color: #ffffff;
        border-color: rgba(56, 189, 248, 0.4);
    }

    .pattern-btn:checked,
    .pattern-btn.active {
        background-color: #2563eb;
        color: #ffffff;
        border-color: #38bdf8;
    }

    /* == Dropdown Selectors == */
    .effect-dropdown {
        background-color: #1a1e30;
        border: 1px solid rgba(255, 255, 255, 0.1);
        border-radius: 8px;
        padding: 4px 8px;
        color: #f1f5f9;
    }

    /* == Power Toggle Buttons == */
    .power-on-btn {
        background-color: #10b981;
        color: #ffffff;
        border-radius: 8px;
        font-weight: 700;
        font-size: 12px;
        padding: 7px 18px;
        border: none;
        box-shadow: 0 2px 10px rgba(16, 185, 129, 0.35);
        transition: background-color 150ms ease;
    }

    .power-on-btn:hover {
        background-color: #059669;
    }

    .power-off-btn {
        background-color: #ef4444;
        color: #ffffff;
        border-radius: 8px;
        font-weight: 700;
        font-size: 12px;
        padding: 7px 18px;
        border: none;
        box-shadow: 0 2px 10px rgba(239, 68, 68, 0.35);
        transition: background-color 150ms ease;
    }

    .power-off-btn:hover {
        background-color: #dc2626;
    }

    /* == Device List in Connection Panel == */
    .device-list {
        background-color: #0b0d14;
        border-radius: 8px;
        border: 1px solid rgba(255, 255, 255, 0.07);
    }

    .device-list row {
        border-radius: 6px;
        margin: 2px 4px;
        padding: 6px 8px;
        transition: background-color 150ms ease;
    }

    .device-list row:selected {
        background-color: #1e293b;
    }

    .device-name {
        font-weight: 600;
        font-size: 13px;
        color: #f8fafc;
    }

    .device-addr {
        font-family: monospace;
        font-size: 11px;
        color: #64748b;
    }

    .device-rssi {
        font-size: 11px;
        font-weight: 600;
        color: #38bdf8;
    }

    /* == Action Buttons == */
    button.suggested-action {
        background-color: #2563eb;
        color: #ffffff;
        font-weight: 600;
        border-radius: 8px;
        padding: 7px 16px;
        border: none;
        box-shadow: 0 2px 8px rgba(37, 99, 235, 0.3);
        transition: background-color 150ms ease;
    }

    button.suggested-action:hover {
        background-color: #1d4ed8;
    }

    button.destructive-action {
        background-color: #991b1b;
        color: #ffffff;
        font-weight: 600;
        border-radius: 8px;
        padding: 7px 16px;
        border: none;
        transition: background-color 150ms ease;
    }

    button.destructive-action:hover {
        background-color: #7f1d1d;
    }

    /* == Progress / VU Meter == */
    progressbar trough {
        background-color: #0b0d14;
        border-radius: 6px;
        min-height: 8px;
    }

    progressbar progress {
        background-color: #00e676;
        border-radius: 6px;
    }
    "#
}
