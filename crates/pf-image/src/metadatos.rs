//! Los metadatos EXIF que la cámara dejó dentro del archivo.

use std::path::Path;

/// Un campo EXIF ya traducido a texto.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CampoExif {
    pub etiqueta: String,
    pub valor: String,
}

/// Lo que PhotoForge enseña del archivo.
///
/// `resumen` son los seis u ocho datos que un fotógrafo mira siempre, sacados
/// aparte para que la interfaz no tenga que buscarlos entre los ciento y pico
/// campos que trae un JPEG de móvil. `completo` es todo lo demás, para quien
/// quiera mirar.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct Metadatos {
    pub camara: Option<String>,
    pub objetivo: Option<String>,
    pub distancia_focal: Option<String>,
    pub diafragma: Option<String>,
    pub obturacion: Option<String>,
    pub iso: Option<String>,
    pub fecha: Option<String>,
    pub completo: Vec<CampoExif>,
}

impl Metadatos {
    /// ¿Trae el archivo algo que enseñar?
    pub fn hay_algo(&self) -> bool {
        !self.completo.is_empty()
    }
}

/// Lee los metadatos de un archivo.
///
/// Por qué NO devuelve `Resultado`: una foto sin EXIF no es un error. Un PNG
/// exportado de cualquier programa no tiene ni uno, y un JPEG al que le han
/// quitado los datos tampoco. Si esto devolviera `Err`, abrir ese archivo
/// fallaría entero por no poder enseñar un panel lateral, que es exactamente
/// la clase de rigidez que hace que un programa sea molesto de usar. Lo que
/// no se pueda leer, sencillamente no sale.
pub fn leer(ruta: &Path) -> Metadatos {
    let Ok(archivo) = std::fs::File::open(ruta) else {
        return Metadatos::default();
    };
    let mut lector = std::io::BufReader::new(archivo);
    let Ok(exif) = exif::Reader::new().read_from_container(&mut lector) else {
        return Metadatos::default();
    };

    let mut m = Metadatos::default();
    for campo in exif.fields() {
        let valor = campo.display_value().with_unit(&exif).to_string();
        let etiqueta = campo.tag.to_string();

        // Se reparten los conocidos al resumen y TODOS van al completo: así la
        // lista larga sigue siendo la verdad del archivo y el resumen es solo
        // un atajo, no una versión recortada que haya que mantener aparte.
        match campo.tag {
            exif::Tag::Model => m.camara = Some(valor.clone()),
            exif::Tag::LensModel => m.objetivo = Some(valor.clone()),
            exif::Tag::FocalLength => m.distancia_focal = Some(valor.clone()),
            exif::Tag::FNumber => m.diafragma = Some(format!("f/{}", valor.trim_start_matches("f/"))),
            exif::Tag::ExposureTime => m.obturacion = Some(valor.clone()),
            exif::Tag::PhotographicSensitivity => m.iso = Some(format!("ISO {valor}")),
            exif::Tag::DateTimeOriginal => m.fecha = Some(valor.clone()),
            _ => {}
        }
        m.completo.push(CampoExif { etiqueta, valor });
    }

    // La marca va delante del modelo solo si el modelo no la repite ya, que es
    // lo normal en Canon («Canon EOS R6») y lo contrario en Samsung («SM-S938B»).
    if let Some(marca) = exif
        .get_field(exif::Tag::Make, exif::In::PRIMARY)
        .map(|f| f.display_value().to_string())
    {
        let marca = marca.trim_matches('"').trim().to_string();
        m.camara = match m.camara.take() {
            Some(modelo) => {
                let modelo = modelo.trim_matches('"').trim().to_string();
                if modelo.to_lowercase().starts_with(&marca.to_lowercase()) {
                    Some(modelo)
                } else {
                    Some(format!("{marca} {modelo}"))
                }
            }
            None => Some(marca),
        };
    }

    m.completo.sort_by(|a, b| a.etiqueta.cmp(&b.etiqueta));
    m
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn un_archivo_que_no_existe_no_revienta_ni_devuelve_error() {
        let m = leer(Path::new("no_existe_este_archivo.jpg"));
        assert!(!m.hay_algo());
        assert!(m.camara.is_none());
    }

    #[test]
    fn un_png_sin_exif_devuelve_metadatos_vacios_y_no_falla() {
        let dir = std::env::temp_dir().join("pf_prueba_exif");
        std::fs::create_dir_all(&dir).unwrap();
        let ruta = dir.join("liso.png");
        image::RgbImage::new(2, 2).save(&ruta).unwrap();

        let m = leer(&ruta);
        assert!(!m.hay_algo());

        std::fs::remove_file(&ruta).ok();
    }
}
