//! Abre una foto de verdad, la revela y mide cada paso.
//!
//! Este ejemplo recorre exactamente el mismo camino que la aplicación —abrir,
//! muestrear la vista, aplicar las operaciones, calcular el histograma,
//! exportar— pero sin ventana, así que se puede correr sobre una carpeta
//! entera y las cifras que saca son las que acaban citadas en el README.
//!
//! ```text
//! cargo run -p pf-document --example revelar --release -- foto.jpg
//! ```
//!
//! Con `--exportar` escribe además un JPEG revelado al lado del original.

use std::path::{Path, PathBuf};
use std::time::Instant;

use pf_core::{Dimensiones, Region};
use pf_document::{Documento, Giro, Operacion, Punto, Revelado};

/// El tamaño de vista contra el que se mide: una ventana de 1.600 × 900 en una
/// pantalla sin escalado. Es el caso normal, no el mejor ni el peor.
const VISTA: (u32, u32) = (1600, 900);

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let exportar = args.iter().any(|a| a == "--exportar");
    // Para medir lo que cuesta el giro por cuadro.
    let girar: Option<Giro> = args.iter().find_map(|a| match a.strip_prefix("--girar=") {
        Some("derecha") => Some(Giro::Derecha),
        Some("media") => Some(Giro::MediaVuelta),
        Some("espejo") => Some(Giro::EspejoHorizontal),
        _ => None,
    });
    // Los dos ajustes son parámetros y no constantes porque un JPEG ya
    // revelado y un TIFF salido de un RAW lineal piden exposiciones muy
    // distintas, y el ejemplo tiene que servir para los dos.
    // Si no se pasan, NO se tocan: un RAW se abre con su exposición de
    // partida ya puesta, y forzar un cero la quitaría —que es lo correcto
    // cuando lo pide el usuario, y justo lo que no se quiere al medir—.
    let valor = |nombre: &str| -> Option<f32> {
        args.iter().find_map(|a| a.strip_prefix(nombre).and_then(|v| v.parse().ok()))
    };
    // --degradado=desde,hasta,ev : una franja horizontal, en fracción del
    // alto, para poder medir lo que cuesta.
    let degradado: Option<(f32, f32, f32)> = args.iter().find_map(|a| {
        let v: Vec<f32> = a.strip_prefix("--degradado=")?.split(',').filter_map(|x| x.parse().ok()).collect();
        if v.len() == 3 { Some((v[0], v[1], v[2])) } else { None }
    });
    let ev = valor("--ev=");
    let contraste = valor("--contraste=");
    // Los cuatro tonales, para poder medir el caso peor: todo movido.
    let luces = valor("--luces=");
    let sombras = valor("--sombras=");
    let blancos = valor("--blancos=");
    let negros = valor("--negros=");
    let rutas: Vec<PathBuf> = args.iter().filter(|a| !a.starts_with("--")).map(PathBuf::from).collect();

    if rutas.is_empty() {
        eprintln!("uso: revelar <foto.jpg> [más fotos…] [--exportar]");
        std::process::exit(2);
    }

    for ruta in rutas {
        println!("\n═══ {}", ruta.display());
        if let Err(e) = revelar(&ruta, exportar, girar, degradado, ev, contraste, [luces, sombras, blancos, negros]) {
            eprintln!("  ERROR: {e}");
        }
    }
}

fn revelar(
    ruta: &Path,
    exportar: bool,
    girar: Option<Giro>,
    degradado: Option<(f32, f32, f32)>,
    ev: Option<f32>,
    contraste: Option<f32>,
    tonales: [Option<f32>; 4],
) -> pf_core::Resultado<()> {
    let reloj = Instant::now();
    let mut doc = Documento::abrir(ruta)?;
    let ms_abrir = reloj.elapsed().as_millis();

    if let Some(g) = girar {
        doc.girar(g);
    }
    if let Some((desde, hasta, ev)) = degradado {
        let d = doc.dim();
        doc.poner_degradado(
            Punto { x: d.ancho as f32 * 0.5, y: d.alto as f32 * desde },
            Punto { x: d.ancho as f32 * 0.5, y: d.alto as f32 * hasta },
            true,
        );
        doc.ajustar_degradado(ev, true);
    }
    let dim = doc.dim();
    println!(
        "  {} × {} ({:.1} Mpx) · {} MB en RGB lineal · abierta en {ms_abrir} ms",
        dim.ancho,
        dim.alto,
        dim.pixeles() as f32 / 1e6,
        doc.bytes_en_memoria() / 1_048_576
    );

    let m = doc.metadatos();
    if m.hay_algo() {
        println!(
            "  EXIF: {} campos · {}",
            m.completo.len(),
            [&m.camara, &m.obturacion, &m.diafragma, &m.iso]
                .iter()
                .filter_map(|x| x.as_deref())
                .collect::<Vec<_>>()
                .join(" · ")
        );
    } else {
        println!("  EXIF: el archivo no trae ninguno");
    }

    let destino = Dimensiones::nueva(VISTA.0, VISTA.1)?;
    let region = Region { x: 0.0, y: 0.0, ancho: dim.ancho as f32, alto: dim.alto as f32 };

    // La vista sin tocar nada: solo recortar y escalar.
    let reloj = Instant::now();
    std::hint::black_box(doc.vista(region, destino, Revelado::ComoSeAbrio));
    let ms_vista = reloj.elapsed().as_millis();

    if let Some(ev) = ev {
        doc.ajustar(Operacion::Exposicion { pasos: ev }, true);
    }
    if let Some(c) = contraste {
        doc.ajustar(Operacion::Contraste { cantidad: c }, true);
    }
    if let Some(c) = tonales[0] {
        doc.ajustar(Operacion::Luces { cantidad: c }, true);
    }
    if let Some(c) = tonales[1] {
        doc.ajustar(Operacion::Sombras { cantidad: c }, true);
    }
    if let Some(c) = tonales[2] {
        doc.ajustar(Operacion::Blancos { cantidad: c }, true);
    }
    if let Some(c) = tonales[3] {
        doc.ajustar(Operacion::Negros { cantidad: c }, true);
    }
    println!("  procedencia: {:?} · ajustes: {:?}", doc.procedencia(), doc.operaciones());

    // Y la vista revelada, que es lo que cuesta cada movimiento del
    // deslizador. Se mide tres veces y se queda con la mediana: la primera
    // paga el reparto de hilos de rayon.
    let mut tiempos = Vec::new();
    for _ in 0..3 {
        let reloj = Instant::now();
        let revelada = doc.vista(region, destino, Revelado::Actual);
        tiempos.push(reloj.elapsed().as_secs_f64() * 1000.0);
        std::hint::black_box(revelada);
    }
    tiempos.sort_by(f64::total_cmp);

    // Y lo que cuesta pasarlo a bytes para el lienzo.
    let revelada = doc.vista(region, destino, Revelado::Actual);
    let reloj = Instant::now();
    let bytes = revelada.a_rgba8();
    let ms_rgba = reloj.elapsed().as_secs_f64() * 1000.0;

    println!("  vista {}×{} sin revelar: {ms_vista} ms", VISTA.0, VISTA.1);
    println!("  vista revelada (mediana de 3): {:.1} ms", tiempos[1]);
    println!("  a RGBA de 8 bits: {ms_rgba:.1} ms ({} KB al lienzo)", bytes.len() / 1024);

    let reloj = Instant::now();
    let h = doc.histograma_rapido(1024);
    let ms_hist_rapido = reloj.elapsed().as_secs_f64() * 1000.0;
    let reloj = Instant::now();
    let hc = doc.histograma();
    let ms_hist_completo = reloj.elapsed().as_secs_f64() * 1000.0;
    println!(
        "  histograma rápido: {ms_hist_rapido:.1} ms ({} muestras) · completo: {ms_hist_completo:.1} ms ({} muestras)",
        h.muestras, hc.muestras
    );
    println!(
        "  recorte sobre la foto entera: {:.2} % en negro, {:.2} % quemado",
        hc.recortados_negros as f64 / hc.muestras.max(1) as f64 * 100.0,
        hc.recortados_blancos as f64 / hc.muestras.max(1) as f64 * 100.0
    );

    // La prueba que importa de verdad: el original sigue intacto.
    let copia = doc.original().datos()[..9].to_vec();
    let _ = doc.completa(Revelado::Actual);
    assert_eq!(&doc.original().datos()[..9], copia.as_slice());
    println!("  el original sigue sin tocarse después de todo lo anterior");

    if exportar {
        let salida = ruta.with_file_name(format!(
            "{}-revelado.jpg",
            ruta.file_stem().unwrap_or_default().to_string_lossy()
        ));
        let reloj = Instant::now();
        let completa = doc.completa(Revelado::Actual);
        let ms_revelar = reloj.elapsed().as_millis();
        let reloj = Instant::now();
        pf_image::exportar::exportar(&completa, &salida, pf_image::exportar::Formato::Jpeg { calidad: 92 })?;
        println!(
            "  exportado a resolución completa: revelado {ms_revelar} ms + escritura {} ms → {}",
            reloj.elapsed().as_millis(),
            salida.display()
        );
    }

    Ok(())
}
