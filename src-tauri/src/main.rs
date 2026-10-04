// Pas de console en version finale
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    kane_explorer_lib::run()
}
