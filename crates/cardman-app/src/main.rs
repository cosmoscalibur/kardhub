//! Cardman desktop application entry point.

use dioxus::prelude::*;

fn main() {
    dioxus::launch(app);
}

/// Root application component.
fn app() -> Element {
    rsx! {
        div {
            class: "cardman-app",
            h1 { "Cardman" }
            p { "Kanban task manager synced with GitHub" }
        }
    }
}
