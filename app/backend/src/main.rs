// Sin consola en Windows en la versión final.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    branchboard_lib::run()
}
