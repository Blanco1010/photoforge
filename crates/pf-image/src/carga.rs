//! Abrir un archivo y convertirlo en `Imagen`.

use std::path::Path;

use image::DynamicImage;
use pf_core::{Dimensiones, Error, Resultado};
use rayon::prelude::*;

use crate::{CANALES, Imagen, color};

/// Extensiones que PhotoForge sabe abrir hoy.
///
/// Está aquí y no repartida por la interfaz para que el diálogo de «Abrir» y
/// la comprobación del backend no puedan desincronizarse. El día que entre
/// RAW, se añade aquí y los dos sitios se enteran.
pub const EXTENSIONES: &[&str] = &["jpg", "jpeg", "png", "tif", "tiff"];

pub fn extension_soportada(ruta: &Path) -> bool {
    ruta.extension()
        .and_then(|e| e.to_str())
        .map(|e| EXTENSIONES.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

/// Carga un archivo y lo deja en RGB lineal.
///
/// Por qué se mira la extensión antes de intentarlo: `image` intenta adivinar
/// el formato por los primeros bytes y, con un archivo que no es una imagen,
/// devuelve un error interno sobre cabeceras que no le dice nada a nadie.
/// Vale más un «esto no es un formato que yo sepa abrir» con el nombre del
/// archivo delante.
pub fn cargar(ruta: &Path) -> Resultado<Imagen> {
    if !extension_soportada(ruta) {
        return Err(Error::FormatoNoSoportado(
            ruta.extension()
                .and_then(|e| e.to_str())
                .unwrap_or("sin extensión")
                .to_string(),
        ));
    }

    let decodificada = image::ImageReader::open(ruta)
        .map_err(|e| Error::Lectura { ruta: ruta.display().to_string(), causa: e.to_string() })?
        .with_guessed_format()
        .map_err(|e| Error::Lectura { ruta: ruta.display().to_string(), causa: e.to_string() })?
        .decode()
        .map_err(|e| Error::Lectura { ruta: ruta.display().to_string(), causa: e.to_string() })?;

    desde_dynamic(decodificada)
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
        assert!(!extension_soportada(Path::new("foto.dng")));
        assert!(!extension_soportada(Path::new("foto")));
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

        let img = cargar(&ruta).unwrap();
        assert_eq!(img.ancho(), 3);
        // El gris 118 tiene que haber entrado como ~0,18 de luz lineal.
        let medio = img.datos()[3];
        assert!((medio - 0.18).abs() < 0.005, "el gris medio entró como {medio}");
        assert_eq!(img.a_rgb8(), vec![0, 0, 0, 118, 118, 118, 255, 255, 255]);

        std::fs::remove_file(&ruta).ok();
    }
}
