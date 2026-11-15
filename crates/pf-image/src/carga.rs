//! Abrir un archivo y convertirlo en `Imagen`.
//!
//! Hay una sola puerta —[`cargar`]— y es ella quien decide si el archivo va
//! por el camino de `image` (JPEG, PNG, TIFF) o por el de `rawler` (RAW de
//! cámara). Quien llama no tiene que saber de qué tipo es el archivo, y por
//! eso no hay dos funciones parecidas que alguien pueda confundir.

use std::path::Path;

use image::DynamicImage;
use pf_core::{Dimensiones, Error, Procedencia, Resultado};
use rayon::prelude::*;

use crate::{CANALES, Imagen, color};

/// Formatos ya revelados que PhotoForge sabe abrir.
pub const EXTENSIONES_REVELADAS: &[&str] = &["jpg", "jpeg", "png", "tif", "tiff"];

/// Todas las extensiones, para el diálogo de abrir.
///
/// Se construye aquí y no en la interfaz para que el filtro del diálogo y la
/// comprobación del backend no puedan desincronizarse: el día que entre un
/// formato nuevo, los dos sitios se enteran de una vez.
pub fn extensiones() -> Vec<&'static str> {
    EXTENSIONES_REVELADAS
        .iter()
        .chain(crate::raw::EXTENSIONES.iter())
        .copied()
        .collect()
}

pub fn extension_soportada(ruta: &Path) -> bool {
    es_revelada(ruta) || crate::raw::es_raw(ruta)
}

fn es_revelada(ruta: &Path) -> bool {
    ruta.extension()
        .and_then(|e| e.to_str())
        .map(|e| EXTENSIONES_REVELADAS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

/// El resultado de abrir un archivo: los píxeles y lo que hace falta saber
/// sobre ellos para tratarlos bien.
#[derive(Debug)]
pub struct Cargada {
    pub imagen: Imagen,
    pub procedencia: Procedencia,
    /// Qué cámara lo hizo, según el propio RAW. Para los archivos ya
    /// revelados esto viene del EXIF y no de aquí.
    pub camara: Option<String>,
    /// Si el archivo traía mosaico de Bayer y hubo que reconstruir el color.
    pub con_mosaico: bool,
    /// Cómo venía girada la foto. Ya está aplicada sobre los píxeles.
    pub orientacion: crate::orientacion::Orientacion,
}

/// Carga un archivo y lo deja en RGB lineal.
///
/// Por qué se mira la extensión antes de intentarlo: `image` adivina el
/// formato por los primeros bytes y, con un archivo que no es una imagen,
/// devuelve un error interno sobre cabeceras que no le dice nada a nadie.
/// Vale más un «esto no es un formato que yo sepa abrir» con el nombre del
/// archivo delante.
pub fn cargar(ruta: &Path) -> Resultado<Cargada> {
    if crate::raw::es_raw(ruta) {
        let (imagen, ficha) = crate::raw::cargar(ruta)?;
        return Ok(Cargada {
            imagen,
            procedencia: Procedencia::Escena,
            camara: Some(ficha.camara).filter(|c| !c.is_empty()),
            con_mosaico: ficha.con_mosaico,
            orientacion: ficha.orientacion,
        });
    }

    if !es_revelada(ruta) {
        return Err(Error::FormatoNoSoportado(
            ruta.extension()
                .and_then(|e| e.to_str())
                .unwrap_or("sin extensión")
                .to_string(),
        ));
    }

    let fallo = |causa: String| Error::Lectura { ruta: ruta.display().to_string(), causa };
    let decodificada = image::ImageReader::open(ruta)
        .map_err(|e| fallo(e.to_string()))?
        .with_guessed_format()
        .map_err(|e| fallo(e.to_string()))?
        .decode()
        .map_err(|e| fallo(e.to_string()))?;

    // El mismo problema que en el RAW: un JPEG de móvil guarda los píxeles
    // siempre en el mismo sentido y apunta aparte cómo hay que girarlos. La
    // caja `image` no lo aplica sola.
    let orientacion = crate::metadatos::orientacion(ruta);
    let imagen = crate::orientacion::enderezar(desde_dynamic(decodificada)?, orientacion)?;

    Ok(Cargada {
        imagen,
        procedencia: Procedencia::Revelada,
        camara: None,
        con_mosaico: false,
        orientacion,
    })
}

/// Convierte lo que devuelve `image` en una `Imagen`.
///
/// Por qué solo dos caminos y no uno por cada variante de `DynamicImage`: los
/// doce tipos que puede devolver `image` se reducen a dos preguntas —¿los
/// canales son de 8 o de 16 bits?— y el propio `image` sabe convertir entre
/// ellos. Escribir doce ramas para ahorrarse una conversión que ocurre una vez
/// al abrir el archivo es optimizar lo que no se mide.
fn desde_dynamic(d: DynamicImage) -> Resultado<Imagen> {
    let dim = Dimensiones::nueva(d.width(), d.height())?;
    let pixeles = dim.pixeles();

    // `bits_per_pixel` cuenta todos los canales juntos, así que 48 es RGB de
    // 16 bits y 64 es RGBA de 16 bits. Cualquier cosa por encima de 32 lleva
    // canales de 16 bits y merece la ruta larga.
    let hondo = d.color().bits_per_pixel() > 32;

    let mut datos = vec![0.0f32; pixeles * CANALES];

    if hondo {
        let rgb = d.into_rgb16();
        let tabla = color::tabla_entrada_16();
        datos
            .par_chunks_mut(CANALES * 4096)
            .zip(rgb.as_raw().par_chunks(CANALES * 4096))
            .for_each(|(salida, entrada)| {
                for (s, e) in salida.iter_mut().zip(entrada) {
                    *s = tabla[*e as usize];
                }
            });
    } else {
        let rgb = d.into_rgb8();
        let tabla = color::tabla_entrada_8();
        datos
            .par_chunks_mut(CANALES * 4096)
            .zip(rgb.as_raw().par_chunks(CANALES * 4096))
            .for_each(|(salida, entrada)| {
                for (s, e) in salida.iter_mut().zip(entrada) {
                    *s = tabla[*e as usize];
                }
            });
    }

    Imagen::nueva(dim, datos)
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn se_reconocen_las_extensiones_sin_importar_las_mayusculas() {
        assert!(extension_soportada(Path::new("foto.JPG")));
        assert!(extension_soportada(Path::new("foto.tiff")));
        assert!(extension_soportada(Path::new("foto.dng")));
        assert!(!extension_soportada(Path::new("foto.pdf")));
        assert!(!extension_soportada(Path::new("foto")));
    }

    #[test]
    fn la_lista_del_dialogo_lleva_los_dos_mundos_y_sin_repetidos() {
        let e = extensiones();
        assert!(e.contains(&"jpg") && e.contains(&"dng"));
        let mut ordenadas = e.clone();
        ordenadas.sort_unstable();
        ordenadas.dedup();
        assert_eq!(ordenadas.len(), e.len(), "hay una extensión repetida en la lista");
    }

    #[test]
    fn un_archivo_que_no_es_imagen_da_un_error_con_su_nombre() {
        let err = cargar(Path::new("documento.pdf")).unwrap_err();
        assert!(err.to_string().contains("pdf"), "el mensaje fue: {err}");
    }

    #[test]
    fn un_png_de_8_bits_entra_en_lineal_y_sale_igual_al_volver() {
        // Se escribe un PNG con tres grises conocidos, se carga y se comprueba
        // que el viaje de ida y vuelta no mueve los bytes. Es la prueba que
        // coge una conversión de color puesta del revés.
        let dir = std::env::temp_dir().join("pf_prueba_carga");
        std::fs::create_dir_all(&dir).unwrap();
        let ruta = dir.join("grises.png");
        let mut buf = image::RgbImage::new(3, 1);
        buf.put_pixel(0, 0, image::Rgb([0, 0, 0]));
        buf.put_pixel(1, 0, image::Rgb([118, 118, 118]));
        buf.put_pixel(2, 0, image::Rgb([255, 255, 255]));
        buf.save(&ruta).unwrap();

        let c = cargar(&ruta).unwrap();
        assert_eq!(c.procedencia, Procedencia::Revelada);
        assert_eq!(c.imagen.ancho(), 3);
        // El gris 118 tiene que haber entrado como ~0,18 de luz lineal.
        let medio = c.imagen.datos()[3];
        assert!((medio - 0.18).abs() < 0.005, "el gris medio entró como {medio}");
        assert_eq!(c.imagen.a_rgb8(), vec![0, 0, 0, 118, 118, 118, 255, 255, 255]);

        std::fs::remove_file(&ruta).ok();
    }
}
