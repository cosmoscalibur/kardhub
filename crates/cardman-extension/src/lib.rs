//! Cardman browser extension entry point.
//!
//! Launches a Dioxus web app inside the extension popup/sidebar.

use dioxus::prelude::*;

/// Extension popup root component.
pub fn app() -> Element {
    rsx! {
        div {
            class: "cardman-extension",
            h1 { "Cardman" }
            p { "Kanban task manager synced with GitHub" }
        }
    }
}

/// Entry point called by the extension runtime.
pub fn main() {
    dioxus::launch(app);
}
