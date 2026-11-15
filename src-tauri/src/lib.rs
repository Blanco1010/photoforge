//! PhotoForge — editor de fotografía no destructivo.
//!
//! Esta caja es solo la cáscara: tiene la ventana, el estado y los comandos
//! que la interfaz puede llamar. Todo lo que sabe de fotografía está en
//! `pf-core`, `pf-image` y `pf-document`, que compilan y se prueban sin Tauri.
//!
//! La regla que mantiene eso en pie: aquí no se escribe ni un cálculo sobre
//! píxeles. Si hace falta uno, su sitio es `pf-image`.

pub mod comandos;
pub mod estado;

use estado::Estado;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            app.manage(Estado::default());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            comandos::abrir,
            comandos::cerrar,
            comandos::ficha,
            comandos::renderizar,
            comandos::ajustar,
            comandos::restablecer,
            comandos::deshacer,
            comandos::rehacer,
            comandos::histograma,
            comandos::exportar,
            comandos::extensiones_soportadas,
        ])
        .run(tauri::generate_context!())
        .expect("error irrecuperable al arrancar PhotoForge");
}
