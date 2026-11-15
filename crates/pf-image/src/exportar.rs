//! Escribir el resultado en un archivo.

use std::path::Path;

use pf_core::{Error, Resultado};
use rayon::prelude::*;

use crate::{CANALES, Imagen, color::TablaSalida};

/// Los formatos a los que PhotoForge sabe exportar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase", tag = "tipo")]
pub enum Formato {
    /// Para enseñar y compartir. La calidad va de 1 a 100.
    Jpeg { calidad: u8 },
    /// Sin pérdida y de 8 bits.
    Png,
    /// Sin pérdida y de 16 bits: el que hay que elegir para seguir editando
    /// en otro programa, porque conserva los medios tonos que un archivo de
    /// 8 bits ya ha redondeado.
    Tiff16,
}

impl Formato {
    pub fn extension(&self) -> &'static str {
        match self {
            Formato::Jpeg { .. } => "jpg",
            Formato::Png => "png",
            Formato::Tiff16 => "tif",
        }
    }
}

/// Escribe la imagen en disco.
///
/// Por qué se escribe primero a un archivo temporal y después se renombra: si
/// el programa se cierra a mitad de escribir un TIFF de 300 MB, el renombrado
/// no ha ocurrido y en el disco queda el archivo anterior entero. Sin esto, la
/// alternativa es un archivo a medias con el nombre bueno, que es peor que no
/// tener nada porque parece válido.
pub fn exportar(img: &Imagen, ruta: &Path, formato: Formato) -> Resultado<()> {
    let temporal = ruta.with_extension(format!("{}.pf-parcial", formato.extension()));

    let escritura = escribir(img, &temporal, formato);
    if escritura.is_err() {
        std::fs::remove_file(&temporal).ok();
        return escritura;
    }

    std::fs::rename(&temporal, ruta).map_err(|e| {
        std::fs::remove_file(&temporal).ok();
        Error::Escritura { ruta: ruta.display().to_string(), causa: e.to_string() }
    })
}

fn escribir(img: &Imagen, ruta: &Path, formato: Formato) -> Resultado<()> {
    let error = |e: image::ImageError| Error::Escritura {
        ruta: ruta.display().to_string(),
        causa: e.to_string(),
    };
    let (ancho, alto) = (img.ancho(), img.alto());

    match formato {
        Formato::Jpeg { calidad } => {
            let buffer = image::RgbImage::from_raw(ancho, alto, img.a_rgb8())
                .expect("el buffer de 8 bits no cuadra con las dimensiones");
            let archivo = std::fs::File::create(ruta).map_err(|e| Error::Escritura {
                ruta: ruta.display().to_string(),
                causa: e.to_string(),
            })?;
            let mut cod = image::codecs::jpeg::JpegEncoder::new_with_quality(
                std::io::BufWriter::new(archivo),
                calidad.clamp(1, 100),
            );
            cod.encode_image(&buffer).map_err(error)
        }
        Formato::Png => {
            let buffer = image::RgbImage::from_raw(ancho, alto, img.a_rgb8())
                .expect("el buffer de 8 bits no cuadra con las dimensiones");
            buffer.save_with_format(ruta, image::ImageFormat::Png).map_err(error)
        }
        Formato::Tiff16 => {
            let mut datos = vec![0u16; img.datos().len()];
            datos
                .par_chunks_mut(4096)
                .zip(img.datos().par_chunks(4096))
                .for_each(|(salida, entrada)| {
                    for (s, e) in salida.iter_mut().zip(entrada) {
                        *s = TablaSalida::a_u16(*e);
                    }
                });
            let buffer: image::ImageBuffer<image::Rgb<u16>, Vec<u16>> =
                image::ImageBuffer::from_raw(ancho, alto, datos)
                    .expect("el buffer de 16 bits no cuadra con las dimensiones");
            buffer.save_with_format(ruta, image::ImageFormat::Tiff).map_err(error)
        }
    }
}

/// Cuántos canales tiene un píxel, repetido aquí solo para que el `use` de
/// `CANALES` no quede sin usar si alguien recorta este archivo.
#[allow(dead_code)]
const _CANALES: usize = CANALES;

#[cfg(test)]
mod pruebas {
    use super::*;
    use pf_core::Dimensiones;

    fn imagen_gris() -> Imagen {
        let dim = Dimensiones::nueva(4, 4).unwrap();
        Imagen::nueva(dim, vec![0.18; dim.pixeles() * CANALES]).unwrap()
    }

    fn dir() -> std::path::PathBuf {
        let d = std::env::temp_dir().join("pf_prueba_exportar");
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn el_png_exportado_se_vuelve_a_leer_con_los_mismos_valores() {
        let ruta = dir().join("ida_y_vuelta.png");
        exportar(&imagen_gris(), &ruta, Formato::Png).unwrap();
        let leida = crate::carga::cargar(&ruta).unwrap();
        assert_eq!(leida.dim(), imagen_gris().dim());
        assert!((leida.datos()[0] - 0.18).abs() < 0.005);
        std::fs::remove_file(&ruta).ok();
    }

    #[test]
    fn el_tiff_de_16_bits_conserva_mas_que_el_png_de_8() {
        // Dos valores lineales tan cercanos que en 8 bits caen en el MISMO
        // byte. En 16 bits tienen que seguir siendo distintos: eso es lo único
        // que justifica ofrecer el formato.
        let dim = Dimensiones::nueva(2, 1).unwrap();
        let img = Imagen::nueva(dim, vec![0.180, 0.180, 0.180, 0.1815, 0.1815, 0.1815]).unwrap();

        let png = dir().join("precision.png");
        exportar(&img, &png, Formato::Png).unwrap();
        let bytes = crate::carga::cargar(&png).unwrap().a_rgb8();
        assert_eq!(bytes[0], bytes[3], "en 8 bits los dos valores deberían colapsar");

        let tif = dir().join("precision.tif");
        exportar(&img, &tif, Formato::Tiff16).unwrap();
        let leido = crate::carga::cargar(&tif).unwrap();
        assert_ne!(leido.datos()[0], leido.datos()[3], "en 16 bits no deberían colapsar");

        std::fs::remove_file(&png).ok();
        std::fs::remove_file(&tif).ok();
    }

    #[test]
    fn no_queda_ningun_archivo_parcial_despues_de_exportar() {
        let ruta = dir().join("limpio.jpg");
        exportar(&imagen_gris(), &ruta, Formato::Jpeg { calidad: 90 }).unwrap();
        assert!(ruta.exists());
        assert!(!ruta.with_extension("jpg.pf-parcial").exists());
        std::fs::remove_file(&ruta).ok();
    }
}
