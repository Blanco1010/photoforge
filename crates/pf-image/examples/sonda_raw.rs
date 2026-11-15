//! Sonda de riesgo de PhotoForge: abrir un RAW, informar y sacar un TIFF.
//!
//! # Qué contesta este programa y por qué existe
//!
//! Todo PhotoForge depende de poder decodificar RAW. Antes de diseñar nada
//! alrededor de esa idea había que comprobarlo, y comprobarlo quiere decir
//! correr código contra archivos reales, no leer documentación.
//!
//! Lo que mide:
//!
//! 1. Que `rawler` decodifica el archivo en Windows con MSVC, sin FFI y sin
//!    compilar nada en C.
//! 2. Qué dice el archivo de sí mismo: si trae mosaico de Bayer o viene ya
//!    lineal. El DNG de Expert RAW de un Samsung Galaxy S25 Ultra es
//!    **lineal de 16 bits**, y aplicarle demosaico sería destrozarlo.
//! 3. Que la normalización de `rawler` no se puede dar por buena: con estos
//!    archivos se la salta en silencio. El porqué completo está en
//!    [`pf_image::raw::normalizar`].
//! 4. Cuánto tarda cada paso y qué exposición de partida sale.
//!
//! # Una sola puerta
//!
//! Esto llama **al mismo código que usa la aplicación**, no a una copia. Si
//! la sonda dice que un archivo se abre bien, la aplicación lo abre bien; si
//! algún día dejaran de coincidir, esta sonda dejaría de servir para nada.
//!
//! ```text
//! cargo run -p pf-image --example sonda_raw --release -- foto.dng [salida.tif]
//! ```

use std::path::Path;
use std::time::Instant;

use pf_image::exportar::Formato;
use pf_image::raw;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(entrada) = args.first() else {
        eprintln!("uso: sonda_raw <archivo.dng> [salida.tif]");
        std::process::exit(2);
    };
    let ruta = Path::new(entrada);

    if !raw::es_raw(ruta) {
        eprintln!("«{entrada}» no tiene extensión de RAW. Las conocidas son: {:?}", raw::EXTENSIONES);
        std::process::exit(2);
    }

    println!("═══ {entrada}");
    let reloj = Instant::now();
    let (imagen, ficha) = match raw::cargar(ruta) {
        Ok(x) => x,
        Err(e) => {
            eprintln!("  {e}");
            std::process::exit(1);
        }
    };
    let ms = reloj.elapsed().as_millis();

    println!("  cámara: {}", if ficha.camara.is_empty() { "(no lo dice)" } else { &ficha.camara });
    println!("  {} · {} MB en RGB lineal · decodificada y revelada en {ms} ms",
        imagen.dim(), imagen.bytes() / 1_048_576);
    println!("  mosaico: {}", if ficha.con_mosaico {
        "sí, hubo que reconstruir el color (Bayer o X-Trans)"
    } else {
        "NO, el archivo ya venía lineal — hacerle demosaico habría sido un error"
    });
    println!("  normalización: {}", if ficha.normalizado_a_mano {
        "la hizo PhotoForge (rawler se la saltaba: ver pf_image::raw::normalizar)"
    } else {
        "ya venía hecha, PhotoForge no tocó nada"
    });

    estadisticas(&imagen);

    let reloj = Instant::now();
    let pasos = raw::exposicion_sugerida(&imagen);
    println!("  exposición de partida: {pasos:+.2} EV (calculada en {} ms)", reloj.elapsed().as_millis());

    let Some(salida) = args.get(1) else {
        println!("  (sin segundo argumento: no se escribe nada)");
        return;
    };

    let reloj = Instant::now();
    if let Err(e) = pf_image::exportar::exportar(&imagen, Path::new(salida), Formato::Tiff16) {
        eprintln!("  {e}");
        std::process::exit(1);
    }
    println!("  TIFF de 16 bits escrito en {} ms: {salida}", reloj.elapsed().as_millis());
    println!("  ojo: ese TIFF lleva los datos SIN la exposición de partida, que es lo que");
    println!("       significa «lineal». Abierto en un visor normal se verá oscuro.");
}

fn estadisticas(img: &pf_image::Imagen) {
    let (mut min, mut max, mut suma) = (f32::MAX, f32::MIN, 0.0f64);
    for v in img.datos() {
        min = min.min(*v);
        max = max.max(*v);
        suma += *v as f64;
    }
    let n = img.datos().len();
    println!("  rango lineal: min={min:.5} max={max:.5} media={:.5}", suma / n as f64);
}
