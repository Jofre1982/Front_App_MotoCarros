use dioxus::prelude::*;

#[component]
pub fn App() -> Element {
    rsx! {
        div {
            h1 { "MotoYa" }
            p { "Frontend en construccion." }
        }
    }
}
