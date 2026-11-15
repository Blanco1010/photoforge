//! Sonda de riesgo de PhotoForge: abrir un RAW y sacar un TIFF de 16 bits.
//!
//! # Qué contesta este programa y por qué existe
//!
//! Todo PhotoForge depende de poder decodificar RAW. Antes de diseñar nada
//! alrededor de esa idea había que comprobarla, y comprobarla quiere decir
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
//!    archivos se la salta en silencio (ver `normalizar` más abajo).
//! 4. Cuánto tarda cada paso.
//!
//! # Cómo correrlo
//!
//! ```text
//! cargo run -p pf-image --example sonda_raw --release -- foto.dng salida.tif
//! ```
//!
//! Sin el segundo argumento solo informa, sin escribir nada.

use std::path::Path;

use rawler::imgop::develop::{Intermediate, ProcessingStep, RawDevelop};
use rawler::rawimage::{RawImage, RawImageData, RawPhotometricInterpretation, WhiteLevel};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(entrada) = args.first() else {
        eprintln!("uso: sonda_raw <archivo.dng> [salida.tif]");
        std::process::exit(2);
    };

    let reloj = std::time::Instant::now();
    let mut raw = match rawler::decode_file(Path::new(entrada)) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("rawler no pudo con «{entrada}»: {e}");
            std::process::exit(1);
        }
    };
    let ms_decodificar = reloj.elapsed().as_millis();

    informar(&raw, ms_decodificar);

    let reloj = std::time::Instant::now();
    let normalizado = normalizar(&mut raw);
    let ms_normalizar = reloj.elapsed().as_millis();
    println!(
        "  normalización: {} en {ms_normalizar} ms",
        if normalizado { "hecha aquí" } else { "no hizo falta" }
    );

    // Revelado SIN demosaico y SIN gamma: lo que sale es RGB lineal, que es
    // exactamente la moneda interna de PhotoForge (ver `pf_image`).
    let reloj = std::time::Instant::now();
    let pasos = [
        ProcessingStep::CropActiveArea,
        ProcessingStep::WhiteBalance,
        ProcessingStep::Calibrate,
        ProcessingStep::CropDefault,
    ];
    let intermedio = match RawDevelop::new_with(&pasos).develop_intermediate(&raw) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("el revelado falló: {e}");
            std::process::exit(1);
        }
    };
    let ms_revelar = reloj.elapsed().as_millis();

    let Intermediate::ThreeColor(pixeles) = intermedio else {
        eprintln!("el revelado no dio tres canales; este ejemplo solo cubre RGB");
        std::process::exit(1);
    };
    let dim = pixeles.dim();
    println!("  revelado lineal: {}×{} en {ms_revelar} ms", dim.w, dim.h);
    estadisticas(pixeles.data.iter().flatten().copied());

    let Some(salida) = args.get(1) else {
        println!("  (sin segundo argumento: no se escribe nada)");
        return;
    };

    // Y ahora el puente con PhotoForge: los mismos píxeles metidos en una
    // `Imagen` y escritos con el exportador de la casa. Que esto funcione es
    // la prueba de que el camino RAW → documento no destructivo está abierto.
    let dimensiones = match pf_core::Dimensiones::nueva(dim.w as u32, dim.h as u32) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    let datos: Vec<f32> = pixeles.data.iter().flatten().copied().collect();
    let imagen = match pf_image::Imagen::nueva(dimensiones, datos) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };

    let reloj = std::time::Instant::now();
    if let Err(e) =
        pf_image::exportar::exportar(&imagen, Path::new(salida), pf_image::exportar::Formato::Tiff16)
    {
        eprintln!("{e}");
        std::process::exit(1);
    }
    println!("  TIFF de 16 bits escrito en {} ms: {salida}", reloj.elapsed().as_millis());
}

fn informar(raw: &RawImage, ms: u128) {
    println!("{} {}", raw.clean_make, raw.clean_model);
    println!("  decodificado en {ms} ms");
    println!("  {}×{}, {} componentes por píxel, {} bits", raw.width, raw.height, raw.cpp, raw.bps);
    match &raw.photometric {
        RawPhotometricInterpretation::Cfa(c) => {
            println!("  mosaico: sí, sensor {:?} — HAY que hacer demosaico", c.sensor);
        }
        RawPhotometricInterpretation::LinearRaw => {
            println!("  mosaico: NO, el archivo ya viene lineal — demosaico sería un error");
        }
        RawPhotometricInterpretation::BlackIsZero => {
            println!("  monocromo");
        }
    }
    println!(
        "  niveles de negro: {} valores (patrón {}×{}, {} canales)",
        raw.blacklevel.levels.len(),
        raw.blacklevel.height,
        raw.blacklevel.width,
        raw.blacklevel.cpp
    );
    println!("  niveles de blanco: {:?}", raw.whitelevel.0);
    println!("  balance de blancos del archivo: {:?}", raw.wb_coeffs);
}

/// Normaliza los datos crudos a 0..1 restando el negro y dividiendo por el
/// rango útil.
///
/// # Por qué PhotoForge hace esto en vez de usar `RawImage::apply_scaling`
///
/// Porque con estos archivos `apply_scaling` no hace nada, y no lo dice.
///
/// Dentro llama a `correct_blacklevel(datos, negros, blancos)`, que empieza
/// comparando `negros.len()` con `blancos.len()`. El DNG del S25 Ultra trae un
/// patrón de negro de 2×2 por 3 canales —**12 valores**— y **3** niveles de
/// blanco. Como 12 ≠ 3, la función cae en su rama final, escribe un
/// `log::warn!` y vuelve sin tocar un solo píxel. Nadie ve ese aviso: en una
/// aplicación de escritorio sin `env_logger` montado no sale por ningún lado.
///
/// El resultado es que los valores siguen entre 0 y 65.535 cuando el resto de
/// la cadena los espera entre 0 y 1. Al pintar, todo lo que pasa de 1 se
/// recorta a blanco: la foto sale **enteramente blanca**. Es un fallo que no
/// se parece a un fallo de escala —parece que la decodificación no funcionó—,
/// y esa es la razón de que este ejemplo exista y de que el código esté aquí
/// en vez de confiarse a la biblioteca.
///
/// Medido sobre los cinco DNG de prueba: todos dan 12 niveles de negro contra
/// 3 de blanco, así que a todos les pasa.
///
/// Devuelve `true` si tuvo que normalizar.
fn normalizar(raw: &mut RawImage) -> bool {
    let blancos: Vec<f32> = raw.whitelevel.0.iter().map(|x| *x as f32).collect();
    if blancos.iter().all(|b| (*b - 1.0).abs() < f32::EPSILON) {
        return false; // ya estaba normalizado
    }

    let cpp = raw.cpp;
    let ancho = raw.width;
    let negros: Vec<f32> = raw.blacklevel.levels.iter().map(rawler::formats::tiff::Rational::as_f32).collect();
    let (patron_ancho, patron_alto, patron_cpp) =
        (raw.blacklevel.width.max(1), raw.blacklevel.height.max(1), raw.blacklevel.cpp.max(1));

    let crudos: Vec<f32> = match &raw.data {
        RawImageData::Integer(v) => v.iter().map(|x| *x as f32).collect(),
        RawImageData::Float(v) => v.clone(),
    };

    let mut datos = vec![0.0f32; crudos.len()];
    for (i, destino) in datos.iter_mut().enumerate() {
        let pixel = i / cpp;
        let canal = i % cpp;
        let (x, y) = (pixel % ancho, pixel / ancho);

        // El patrón de negro se repite en mosaico por toda la imagen.
        let indice_negro =
            ((y % patron_alto) * patron_ancho + (x % patron_ancho)) * patron_cpp + canal.min(patron_cpp - 1);
        let negro = negros.get(indice_negro).copied().unwrap_or(0.0);
        let blanco = blancos.get(canal).copied().unwrap_or_else(|| blancos[0]);

        let rango = (blanco - negro).max(1.0);
        *destino = ((crudos[i] - negro) / rango).max(0.0);
    }

    raw.data = RawImageData::Float(datos);
    raw.whitelevel = WhiteLevel(vec![1; cpp]);
    true
}

fn estadisticas(valores: impl Iterator<Item = f32>) {
    let (mut min, mut max, mut suma, mut n) = (f32::MAX, f32::MIN, 0.0f64, 0u64);
    for v in valores {
        min = min.min(v);
        max = max.max(v);
        suma += v as f64;
        n += 1;
    }
    if n == 0 {
        return;
    }
    println!(
        "  rango lineal: min={min:.5} max={max:.5} media={:.5}  ({n} valores)",
        suma / n as f64
    );
    if max < 0.5 {
        println!(
            "  nota: la escena no se acerca al nivel de blanco declarado. Es normal en un RAW\n\
             \x20       bien expuesto, y es el motivo de que un revelador necesite una curva de\n\
             \x20       tono: en luz lineal, una foto correcta se ve oscura."
        );
    }
}
