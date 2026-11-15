// Evita que Windows abra una consola detrás de la ventana en release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    photoforge_lib::run()
}
