//! Prueba del flujo completo, sin ventana.
//!
//! Recorre lo mismo que hace el usuario —abrir, mover un deslizador, pedir el
//! cuadro para el lienzo, deshacer, exportar— pero contra el `Estado` directo
//! en vez de contra los comandos de Tauri. Los comandos son envoltorios de
//! tres líneas sobre esto; lo que puede romperse de verdad está aquí.
//!
//! Por qué no se prueban los comandos tal cual: reciben un `tauri::State`, que
//! solo existe dentro de una aplicación arrancada. Montar una ventana para
//! probar que `ajustar` llama a `Documento::ajustar` sería pagar un minuto de
//! compilación por cada vuelta para comprobar lo que ya se ve leyendo.

use std::path::PathBuf;

use pf_core::{Dimensiones, Region};
use pf_document::{Documento, Operacion, Revelado};
use photoforge_lib::estado::Estado;

/// Fabrica una foto de prueba con un degradado reconocible.
fn foto(nombre: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("pf_prueba_flujo");
    std::fs::create_dir_all(&dir).unwrap();
    let ruta = dir.join(nombre);
    let mut buf = image::RgbImage::new(64, 48);
    for (x, y, p) in buf.enumerate_pixels_mut() {
        *p = image::Rgb([(x * 4) as u8, (y * 5) as u8, 128]);
    }
    buf.save(&ruta).unwrap();
    ruta
}

#[test]
fn sin_documento_abierto_todo_falla_con_el_mismo_error_y_sin_panico() {
    let estado = Estado::default();
    assert!(!estado.hay_documento());

    let r = estado.con(|d| d.dim());
    assert!(r.is_err());
    assert_eq!(r.unwrap_err().to_string(), "no hay ninguna foto abierta");

    let r = estado.con_mut(|d| d.deshacer());
    assert!(r.is_err());
}

#[test]
fn el_flujo_de_una_sesion_entera_funciona() {
    let ruta = foto("sesion.png");
    let estado = Estado::default();
    estado.poner(Documento::abrir(&ruta).unwrap());
    assert!(estado.hay_documento());

    // 1. Lo primero que hace la interfaz: pedir el cuadro para el lienzo.
    let destino = Dimensiones::nueva(200, 150).unwrap();
    let region = Region { x: 0.0, y: 0.0, ancho: 64.0, alto: 48.0 };
    let sin_tocar = estado
        .con(|d| d.vista(region, destino, Revelado::Actual).a_rgba8())
        .unwrap();
    assert_eq!(sin_tocar.len(), 200 * 150 * 4, "el lienzo pide RGBA del tamaño exacto");
    assert!(sin_tocar.chunks(4).all(|p| p[3] == 255), "el alfa tiene que salir opaco");

    // 2. El usuario mueve la exposición.
    estado
        .con_mut(|d| d.ajustar(Operacion::Exposicion { pasos: 1.0 }, true))
        .unwrap();
    let revelado = estado
        .con(|d| d.vista(region, destino, Revelado::Actual).a_rgba8())
        .unwrap();
    assert_ne!(revelado, sin_tocar, "subir un paso de exposición tiene que cambiar el cuadro");

    // 3. Y pulsa «Antes»: tiene que volver exactamente al cuadro de partida.
    let comparando = estado
        .con(|d| d.vista(region, destino, Revelado::ComoSeAbrio).a_rgba8())
        .unwrap();
    assert_eq!(comparando, sin_tocar, "el «antes» tiene que ser el cuadro original clavado");

    // 4. Deshace.
    estado.con_mut(|d| d.deshacer()).unwrap();
    let deshecho = estado
        .con(|d| d.vista(region, destino, Revelado::Actual).a_rgba8())
        .unwrap();
    assert_eq!(deshecho, sin_tocar);

    // 5. Y el archivo de origen sigue byte a byte como estaba.
    let antes = std::fs::read(&ruta).unwrap();
    estado
        .con_mut(|d| d.ajustar(Operacion::Contraste { cantidad: 60.0 }, true))
        .unwrap();
    let _ = estado.con(|d| d.completa(Revelado::Actual)).unwrap();
    assert_eq!(std::fs::read(&ruta).unwrap(), antes, "el archivo original se ha tocado");

    // 6. Cerrar libera el documento.
    estado.cerrar();
    assert!(!estado.hay_documento());

    std::fs::remove_file(&ruta).ok();
}

#[test]
fn el_degradado_recorre_su_ciclo_entero_contra_el_estado_real() {
    use pf_document::Punto;

    let ruta = foto("degradado_flujo.png");
    let estado = Estado::default();
    estado.poner(Documento::abrir(&ruta).unwrap());

    let destino = Dimensiones::nueva(64, 48).unwrap();
    let region = Region { x: 0.0, y: 0.0, ancho: 64.0, alto: 48.0 };
    let limpio = estado
        .con(|d| d.vista(region, destino, Revelado::Actual).a_rgba8())
        .unwrap();

    // Poner: la mitad de arriba se oscurece.
    let desde = Punto { x: 32.0, y: 0.0 };
    let hasta = Punto { x: 32.0, y: 48.0 };
    estado.con_mut(|d| d.poner_degradado(desde, hasta, true)).unwrap();
    let con = estado
        .con(|d| d.vista(region, destino, Revelado::Actual).a_rgba8())
        .unwrap();
    assert_ne!(con, limpio, "el degradado tenía que cambiar la vista");

    // Los puntos que salen por la ficha son los mismos que entraron.
    let v = estado.con(|d| d.degradado_en_la_vista()).unwrap().unwrap();
    assert!((v.desde.y - desde.y).abs() < 1e-3 && (v.hasta.y - hasta.y).abs() < 1e-3);

    // Ajustar la luz.
    estado.con_mut(|d| d.ajustar_degradado(-3.0, true)).unwrap();
    let mas = estado
        .con(|d| d.vista(region, destino, Revelado::Actual).a_rgba8())
        .unwrap();
    assert_ne!(mas, con);

    // Deshacer devuelve el ajuste de la luz, no la franja.
    estado.con_mut(|d| d.deshacer()).unwrap();
    assert!(estado.con(|d| d.tiene_degradado()).unwrap());

    // Y quitarlo deja la vista como estaba.
    estado.con_mut(|d| d.quitar_degradado()).unwrap();
    assert!(!estado.con(|d| d.tiene_degradado()).unwrap());
    assert_eq!(
        estado.con(|d| d.vista(region, destino, Revelado::Actual).a_rgba8()).unwrap(),
        limpio
    );

    std::fs::remove_file(&ruta).ok();
}

#[test]
fn exportar_escribe_un_archivo_que_se_puede_volver_a_abrir() {
    let ruta = foto("exportable.png");
    let estado = Estado::default();
    estado.poner(Documento::abrir(&ruta).unwrap());
    estado
        .con_mut(|d| d.ajustar(Operacion::Exposicion { pasos: -0.5 }, true))
        .unwrap();

    let salida = ruta.with_file_name("exportable-revelado.tif");
    let imagen = estado.con(|d| d.completa(Revelado::Actual)).unwrap();
    pf_image::exportar::exportar(&imagen, &salida, pf_image::exportar::Formato::Tiff16).unwrap();

    let reabierto = Documento::abrir(&salida).unwrap();
    assert_eq!(reabierto.dim(), imagen.dim());
    assert!(!reabierto.hay_cambios(), "lo exportado se abre como un documento limpio");

    std::fs::remove_file(&ruta).ok();
    std::fs::remove_file(&salida).ok();
}
