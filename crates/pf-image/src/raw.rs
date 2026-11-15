//! Abrir RAW de cámara.
//!
//! # Lo que esta caja hace y lo que deja hacer a `rawler`
//!
//! Decodificar un RAW son miles de líneas por fabricante: compresiones
//! propietarias, tablas de linealización, tamaños de bloque distintos en cada
//! modelo. Eso lo hace [`rawler`], que es el motor de DNGLab y lleva años
//! recibiendo archivos de gente real.
//!
//! Lo que PhotoForge hace por su cuenta es la **normalización**, y no por
//! gusto: ver [`normalizar`].

use std::path::Path;

use pf_core::{Dimensiones, Error, Resultado};
use rawler::imgop::develop::{Intermediate, ProcessingStep, RawDevelop};
use rawler::rawimage::{RawImage, RawImageData, RawPhotometricInterpretation, WhiteLevel};

use crate::orientacion::{self, Orientacion};
use crate::{CANALES, Imagen};

/// Extensiones de RAW que se ofrecen en el diálogo de abrir.
///
/// Son las que `rawler` dice saber leer. Honestamente: de todas ellas, las
/// únicas probadas contra archivos reales en este proyecto son los DNG de
/// Expert RAW de un Galaxy S25 Ultra. Las demás pasan exactamente por el
/// mismo código, que es el que usa DNGLab a diario, pero aquí no se han
/// podido comprobar y el README lo dice igual de claro.
pub const EXTENSIONES: &[&str] = &[
    "dng", "cr2", "cr3", "nef", "nrw", "arw", "srf", "sr2", "raf", "rw2", "orf", "pef", "iiq",
    "3fr", "mrw", "erf", "kdc", "dcr", "mos", "raw",
];

pub fn es_raw(ruta: &Path) -> bool {
    ruta.extension()
        .and_then(|e| e.to_str())
        .map(|e| EXTENSIONES.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

/// Lo que el archivo RAW cuenta de sí mismo, aparte de los píxeles.
#[derive(Debug, Clone)]
pub struct FichaRaw {
    pub camara: String,
    /// `true` si el archivo trae mosaico de Bayer y hubo que hacer demosaico.
    pub con_mosaico: bool,
    /// Cómo venía girada la foto en el archivo. Ya está aplicada; se informa
    /// para poder decirlo en la ficha.
    pub orientacion: Orientacion,
    /// `true` si la normalización la tuvo que hacer PhotoForge porque
    /// `rawler` se la saltó. Ver [`normalizar`].
    pub normalizado_a_mano: bool,
}

/// Abre un RAW y lo deja en RGB lineal, listo para el documento.
///
/// Lo que sale de aquí es **referido a escena**: son fotones medidos, no un
/// brillo pensado para una pantalla. Por eso quien lo abre tiene que ponerle
/// una exposición de partida; ver [`exposicion_sugerida`].
pub fn cargar(ruta: &Path) -> Resultado<(Imagen, FichaRaw)> {
    let fallo = |causa: String| Error::Lectura { ruta: ruta.display().to_string(), causa };

    let mut raw = rawler::decode_file(ruta).map_err(|e| fallo(e.to_string()))?;

    let con_mosaico = matches!(raw.photometric, RawPhotometricInterpretation::Cfa(_));
    let camara = format!("{} {}", raw.clean_make, raw.clean_model).trim().to_string();
    let normalizado_a_mano = normalizar(&mut raw);

    // Los pasos del revelado de `rawler`, elegidos uno a uno:
    //
    // - `Rescale` NO está: la normalización ya se hizo arriba, y volver a
    //   hacerla dividiría por 1 en el mejor caso y rompería en el peor.
    // - `Demosaic` sí: es lo que reconstruye el color de un sensor Bayer. En
    //   un archivo lineal, como el DNG del S25, `rawler` lo salta solo.
    // - `WhiteBalance` y `Calibrate` sí: aplican el balance que midió la
    //   cámara y la matriz de color del perfil del archivo. Sin ellos la foto
    //   sale con una dominante verde, que es el color crudo de cualquier
    //   sensor.
    // - `SRgb` NO: ese paso mete la curva de la pantalla, y aquí dentro todo
    //   vive en lineal. La curva se pone al final, al pintar.
    let pasos = [
        ProcessingStep::Demosaic,
        ProcessingStep::FujiRotate,
        ProcessingStep::CropActiveArea,
        ProcessingStep::WhiteBalance,
        ProcessingStep::Calibrate,
        ProcessingStep::CropDefault,
    ];
    let intermedio = RawDevelop::new_with(&pasos)
        .develop_intermediate(&raw)
        .map_err(|e| fallo(format!("el revelado falló: {e}")))?;

    let dim = intermedio.dim();
    let datos: Vec<f32> = match intermedio {
        Intermediate::ThreeColor(p) => p.data.iter().flatten().copied().collect(),
        // Una cámara monocroma da un solo canal. Se reparte en los tres para
        // que el resto del programa no tenga que saber que existe ese caso.
        Intermediate::Monochrome(p) => p.data.iter().flat_map(|v| [*v, *v, *v]).collect(),
        Intermediate::FourColor(p) => {
            // Cuatro canales (sensores con amarillo o esmeralda) ya deberían
            // haber pasado a tres en `Calibrate`. Si llegan aquí, es que ese
            // paso no supo con qué matriz hacerlo.
            return Err(fallo(format!(
                "el sensor de esta cámara da cuatro colores y PhotoForge todavía no \
                 sabe pasarlos a RGB ({}×{})",
                p.dim().w,
                p.dim().h
            )));
        }
    };

    let dimensiones = Dimensiones::nueva(dim.w as u32, dim.h as u32)?;
    let imagen = Imagen::nueva(dimensiones, datos)?;

    // La cámara no gira los píxeles al girar el teléfono: los guarda siempre
    // igual y anota cómo hay que verlos. `rawler` lee esa anotación pero no
    // la aplica, así que sin esto la mitad de las fotos salen tumbadas.
    let orientacion = desde_rawler(raw.orientation);
    let imagen = orientacion::enderezar(imagen, orientacion)?;

    Ok((imagen, FichaRaw { camara, con_mosaico, orientacion, normalizado_a_mano }))
}

/// Traduce la orientación de `rawler` a la de PhotoForge.
///
/// Las dos son la misma cosa —los ocho valores de la etiqueta EXIF 0x0112—,
/// pero `rawler` no expone el número, así que hay que pasar por los nombres.
fn desde_rawler(o: rawler::Orientation) -> Orientacion {
    use rawler::Orientation as R;
    Orientacion::desde_exif(match o {
        R::Normal | R::Unknown => 1,
        R::HorizontalFlip => 2,
        R::Rotate180 => 3,
        R::VerticalFlip => 4,
        R::Transpose => 5,
        R::Rotate90 => 6,
        R::Transverse => 7,
        R::Rotate270 => 8,
    })
}

/// Normaliza los valores crudos a 0..1 restando el negro y dividiendo por el
/// rango útil.
///
/// # Por qué esto no se le deja a `rawler`
///
/// Porque con estos archivos su `RawImage::apply_scaling` no hace nada, y no
/// lo dice.
///
/// Dentro llama a `correct_blacklevel(datos, negros, blancos)`, que lo primero
/// que hace es comparar `negros.len()` con `blancos.len()`. El DNG de Expert
/// RAW del Galaxy S25 Ultra trae un patrón de negro de 2×2 por 3 canales
/// —**12 valores**— y **3** niveles de blanco. Como 12 ≠ 3, la función cae en
/// su rama final, escribe un `log::warn!` y vuelve **sin tocar un píxel**.
///
/// Nadie ve ese aviso: una aplicación de escritorio sin `env_logger` montado
/// no lo saca por ningún sitio. Los valores se quedan entre 0 y 65.535 cuando
/// el resto de la cadena los espera entre 0 y 1, y al pintar todo lo que pasa
/// de 1 se recorta: la foto sale **enteramente blanca**. No se parece a un
/// fallo de escala, parece que la decodificación no funcionó.
///
/// Los cinco archivos de prueba dan 12 contra 3, así que les pasa a todos.
///
/// # Si algún día se actualiza `rawler`
///
/// Puede que lo arreglen. Esta función comprueba antes si los niveles de
/// blanco ya valen 1 —que es lo que deja `apply_scaling` cuando SÍ hace su
/// trabajo— y en ese caso no toca nada. Así, normalizar dos veces es
/// imposible por construcción y no por acordarse.
///
/// Devuelve `true` si tuvo que normalizar.
pub fn normalizar(raw: &mut RawImage) -> bool {
    let blancos: Vec<f32> = raw.whitelevel.0.iter().map(|x| *x as f32).collect();
    if blancos.is_empty() || blancos.iter().all(|b| *b <= 1.0) {
        return false;
    }

    let cpp = raw.cpp.max(1);
    let ancho = raw.width.max(1);
    let negros: Vec<f32> = raw.blacklevel.as_vec();
    let patron_ancho = raw.blacklevel.width.max(1);
    let patron_alto = raw.blacklevel.height.max(1);
    let patron_cpp = raw.blacklevel.cpp.max(1);

    let crudos: Vec<f32> = match &raw.data {
        RawImageData::Integer(v) => v.iter().map(|x| *x as f32).collect(),
        RawImageData::Float(v) => v.clone(),
    };

    let mut datos = vec![0.0f32; crudos.len()];
    for (i, destino) in datos.iter_mut().enumerate() {
        let pixel = i / cpp;
        let canal = i % cpp;
        let (x, y) = (pixel % ancho, pixel / ancho);

        // El patrón de niveles de negro se repite en mosaico por toda la
        // imagen: un sensor no tiene el mismo suelo en los píxeles rojos que
        // en los verdes, y el DNG lo dice con una rejilla pequeña.
        let indice = ((y % patron_alto) * patron_ancho + (x % patron_ancho)) * patron_cpp
            + canal.min(patron_cpp - 1);
        let negro = negros.get(indice).copied().unwrap_or(0.0);
        let blanco = blancos.get(canal).copied().unwrap_or(blancos[0]);

        // El `max(1.0)` protege de un archivo con el blanco por debajo del
        // negro, que daría una división por cero o una imagen invertida.
        let rango = (blanco - negro).max(1.0);
        *destino = ((crudos[i] - negro) / rango).max(0.0);
    }

    raw.data = RawImageData::Float(datos);
    raw.whitelevel = WhiteLevel(vec![1; cpp]);
    true
}

/// Dónde se quiere que caiga el percentil alto de la foto.
///
/// 0,72 y no 1,0: dejar la luz más alta pegada al blanco no deja sitio para
/// subir la exposición después, y las fotos acaban pareciendo planas. Queda
/// justo por debajo de [`crate::color::CODO_DE_LUCES`], así que la foto de
/// partida no está tocando la curva de luces todavía.
const OBJETIVO: f32 = 0.72;

/// Qué parte de la foto puede quedar por encima del objetivo.
///
/// El 0,5 % más brillante se deja fuera del cálculo a propósito: una farola,
/// un reflejo en el agua o un trozo de cielo entre las hojas son puntos que
/// DEBEN quemarse. Si contaran, bastaría un reflejo para dejar toda la foto
/// subexpuesta, que es el fallo clásico de las exposiciones automáticas.
const PERCENTIL: f32 = 0.995;

/// Cuánta exposición hace falta para que un RAW se vea.
///
/// # Por qué un RAW necesita esto
///
/// Los valores de un RAW son fotones medidos contra el techo del sensor, y
/// una foto bien expuesta usa una fracción pequeña de ese techo: medido sobre
/// cinco DNG del S25 Ultra, los máximos van de 1.334 a 29.352 sobre un techo
/// declarado de 65.535. Enseñar eso tal cual da una foto oscura, y el usuario
/// pensaría que algo se rompió en vez de que falta revelarla.
///
/// # Por qué un percentil y no la media
///
/// La media la arrastra lo que más superficie ocupa. Una foto con fondo negro
/// saldría quemada y una con nieve, gris. El percentil alto mira dónde están
/// las luces, que es lo que de verdad no se puede recuperar.
///
/// # Por qué sale como una exposición normal y no como magia escondida
///
/// Lo que devuelve esta función acaba en el deslizador de exposición, a la
/// vista, movible y con su «0» al lado. PhotoForge no tiene un ajuste oculto
/// que el usuario no pueda ver ni quitar.
pub fn exposicion_sugerida(img: &Imagen) -> f32 {
    // Se mira como mucho un millón de píxeles, repartidos por toda la foto.
    // Con un millón de muestras el percentil 99,5 es estable en la tercera
    // cifra, y recorrer cincuenta millones para afinar la cuarta no cambia
    // ningún deslizador.
    let total = img.dim().pixeles();
    let salto = (total / 1_000_000).max(1);
    let datos = img.datos();

    let mut luces: Vec<f32> = Vec::with_capacity(total / salto + 1);
    for p in (0..total).step_by(salto) {
        let i = p * CANALES;
        // Rec. 709 sobre valores lineales, que es donde esos coeficientes
        // tienen sentido físico.
        luces.push(0.2126 * datos[i] + 0.7152 * datos[i + 1] + 0.0722 * datos[i + 2]);
    }
    if luces.is_empty() {
        return 0.0;
    }

    // `select_nth_unstable` coloca el elemento que toca sin ordenar el resto:
    // es O(n) en vez de O(n log n), y aquí n es un millón.
    let k = ((luces.len() - 1) as f32 * PERCENTIL) as usize;
    let (_, alto, _) = luces.select_nth_unstable_by(k, f32::total_cmp);
    let alto = *alto;

    // Una foto negra entera no tiene exposición que sugerir. Devolver 0 es
    // más honrado que inventar +20 EV y enseñar ruido amplificado.
    //
    // El `!(x > y)` en vez de `x <= y` es a propósito, y clippy se queja: un
    // NaN no es menor ni igual que nada, así que con `<=` se colaría y daría
    // una exposición NaN que tiñe la foto entera de negro sin motivo visible.
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    if !(alto > 1e-6) {
        return 0.0;
    }

    // El tope existe porque la exposición multiplica también el ruido: por
    // encima de 6 pasos lo que sale no es una foto rescatada, es grano.
    (OBJETIVO / alto).log2().clamp(-3.0, 6.0)
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn imagen_gris(valor: f32, n: u32) -> Imagen {
        let dim = Dimensiones::nueva(n, 1).unwrap();
        Imagen::nueva(dim, vec![valor; dim.pixeles() * CANALES]).unwrap()
    }

    #[test]
    fn se_reconocen_las_extensiones_de_raw_sin_importar_las_mayusculas() {
        assert!(es_raw(Path::new("foto.DNG")));
        assert!(es_raw(Path::new("foto.cr3")));
        assert!(!es_raw(Path::new("foto.jpg")));
        assert!(!es_raw(Path::new("foto")));
    }

    #[test]
    fn la_exposicion_sugerida_lleva_el_gris_al_objetivo() {
        // Una imagen de un solo tono: su percentil es ese tono, así que la
        // exposición sugerida tiene que colocarlo justo en el objetivo.
        // Tonos que no chocan con el tope de 6 pasos: ese caso tiene su
        // propia prueba, y mezclarlos escondería cuál de las dos cosas falla.
        for tono in [0.02f32, 0.05, 0.2, 0.5, 2.0] {
            let pasos = exposicion_sugerida(&imagen_gris(tono, 1000));
            let resultado = tono * 2.0f32.powf(pasos);
            assert!(
                (resultado - OBJETIVO).abs() < 0.01,
                "con {tono} y {pasos} pasos salió {resultado}"
            );
        }
    }

    #[test]
    fn un_reflejo_suelto_no_subexpone_la_foto_entera() {
        // 999 píxeles a 0,1 y uno a 1,0, que es una farola. Si el cálculo
        // usara el máximo, sugeriría bajar la exposición y dejaría la foto a
        // oscuras. Con el percentil 99,5 tiene que ignorar la farola.
        let dim = Dimensiones::nueva(1000, 1).unwrap();
        let mut datos = vec![0.1f32; dim.pixeles() * CANALES];
        for c in datos.iter_mut().take(CANALES) {
            *c = 1.0;
        }
        let img = Imagen::nueva(dim, datos).unwrap();
        let pasos = exposicion_sugerida(&img);
        assert!(pasos > 2.5, "con una sola farola solo sugirió {pasos} pasos");
    }

    #[test]
    fn una_imagen_negra_no_sugiere_amplificar_el_ruido() {
        assert_eq!(exposicion_sugerida(&imagen_gris(0.0, 100)), 0.0);
    }

    #[test]
    fn la_exposicion_sugerida_esta_acotada_por_los_dos_lados() {
        // Casi negra: querría +20 pasos y tiene que quedarse en 6.
        assert_eq!(exposicion_sugerida(&imagen_gris(1e-5, 100)), 6.0);
        // Quemada: querría bajar mucho y tiene que quedarse en -3.
        assert_eq!(exposicion_sugerida(&imagen_gris(50.0, 100)), -3.0);
    }
}
