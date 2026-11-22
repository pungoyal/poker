//! The trainer's table for the browser. [`Table`] holds a session and turns
//! commands (deal, act, answer a question) into a view for the page to draw;
//! this file is only the WebAssembly boundary.

mod table;

pub use table::Table;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console, js_name = error)]
    fn console_error(text: &str);
}

/// The table as the page's worker sees it: JSON in, JSON out.
#[wasm_bindgen]
pub struct WebTable {
    table: Table,
}

#[wasm_bindgen]
impl WebTable {
    #[wasm_bindgen(constructor)]
    #[allow(clippy::new_without_default)]
    pub fn new() -> WebTable {
        std::panic::set_hook(Box::new(|info| console_error(&info.to_string())));
        WebTable {
            table: Table::default(),
        }
    }

    /// Runs one command (a JSON object with a `cmd`) and returns the reply:
    /// the new view, and whatever the page has to store.
    pub fn command(&mut self, json: &str) -> String {
        self.table.command_json(json)
    }
}
