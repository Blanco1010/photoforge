//! Lo que la interfaz puede pedirle al núcleo.
//!
//! # El cuello de botella y cómo se esquiva
//!
//! Entre la interfaz y Rust hay un puente que, por omisión, serializa a JSON.
//! Un lienzo de 1.600 × 900 en RGBA son 5,7 MB de bytes; en JSON, con un
//! número y una coma por byte, pasarían de 20 MB de texto que hay que
//! construir, copiar, enviar y analizar. A treinta repintados por segundo eso
//! no se sostiene de ninguna manera.
//!
//! Por eso `renderizar` devuelve `tauri::ipc::Response`, que manda los bytes
//! en crudo y llega al navegador como un `ArrayBuffer` que `putImageData`
//! puede usar casi sin tocarlo. Es la única razón por la que este comando no
//! se parece a los demás.

use std::path::PathBuf;

use pf_core::{Dimensiones, Error, Region, Resultado};
use pf_document::{Documento, Operacion};
use pf_image::histograma::Histograma;
use pf_image::metadatos::Metadatos;

use crate::estado::Estado;

/// Todo lo que la interfaz necesita saber de la foto abierta.
///
/// Por qué va todo junto en una estructura en vez de en seis comandos: cada
/// paso por el puente cuesta, y sobre todo cada uno puede devolver un estado
/// distinto del anterior si algo cambió por el medio. Con una sola ficha, lo
/// que enseña la interfaz es siempre coherente consigo mismo.
#[derive(serde::Serialize)]
pub struct Ficha {
    pub ruta: String,
    pub nombre: String,
    pub ancho: u32,
    pub alto: u32,
    pub megapixeles: f32,
    pub megabytes_en_memoria: u32,
    pub metadatos: Metadatos,
    pub operaciones: Vec<Operacion>,
    pub puede_deshacer: bool,
    pub puede_rehacer: bool,
    pub hay_cambios: bool,
}

fn ficha_de(d: &Documento) -> Ficha {
    let dim = d.dim();
    Ficha {
        ruta: d.ruta().display().to_string(),
        nombre: d
            .ruta()
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        ancho: dim.ancho,
        alto: dim.alto,
        megapixeles: dim.pixeles() as f32 / 1_000_000.0,
        megabytes_en_memoria: (d.bytes_en_memoria() / 1_048_576) as u32,
        metadatos: d.metadatos().clone(),
        operaciones: d.operaciones().to_vec(),
        puede_deshacer: d.puede_deshacer(),
        puede_rehacer: d.puede_rehacer(),
        hay_cambios: d.hay_cambios(),
    }
}

#[tauri::command]
pub fn abrir(estado: tauri::State<'_, Estado>, ruta: String) -> Resultado<Ficha> {
    let documento = Documento::abrir(&PathBuf::from(&ruta))?;
    let ficha = ficha_de(&documento);
    estado.poner(documento);
    Ok(ficha)
}

#[tauri::command]
pub fn cerrar(estado: tauri::State<'_, Estado>) {
    estado.cerrar();
}

#[tauri::command]
pub fn ficha(estado: tauri::State<'_, Estado>) -> Resultado<Ficha> {
    estado.con(ficha_de)
}

#[tauri::command]
pub fn extensiones_soportadas() -> Vec<&'static str> {
    pf_image::carga::EXTENSIONES.to_vec()
}

/// Lo que la interfaz pide para pintar un cuadro.
///
/// La región va en coordenadas de la imagen; el tamaño, en píxeles del
/// lienzo. Quien decide las dos cosas es la interfaz, porque es quien sabe
/// cuánto mide su ventana y dónde está el zoom.
#[derive(serde::Deserialize)]
pub struct Peticion {
    pub x: f32,
    pub y: f32,
    pub ancho_region: f32,
    pub alto_region: f32,
    pub ancho: u32,
    pub alto: u32,
    pub con_operaciones: bool,
}

/// Tope del lienzo que se acepta.
///
/// Una petición disparatada —por un error de cálculo del zoom, por ejemplo—
/// reservaría gigabytes antes de que nadie se diera cuenta. 8K de ancho cubre
/// cualquier pantalla real con holgura.
const LADO_MAXIMO: u32 = 8192;

#[tauri::command]
pub fn renderizar(
    estado: tauri::State<'_, Estado>,
    peticion: Peticion,
) -> Resultado<tauri::ipc::Response> {
    let destino = Dimensiones::nueva(
        peticion.ancho.min(LADO_MAXIMO),
        peticion.alto.min(LADO_MAXIMO),
    )?;
    let region = Region {
        x: peticion.x,
        y: peticion.y,
        ancho: peticion.ancho_region.max(1.0),
        alto: peticion.alto_region.max(1.0),
    };

    let bytes = estado.con(|d| d.vista(region, destino, peticion.con_operaciones).a_rgba8())?;
    Ok(tauri::ipc::Response::new(bytes))
}

#[tauri::command]
pub fn ajustar(estado: tauri::State<'_, Estado>, operacion: Operacion) -> Resultado<Ficha> {
    estado.con_mut(|d| {
        d.ajustar(operacion);
        ficha_de(d)
    })
}

#[tauri::command]
pub fn restablecer(estado: tauri::State<'_, Estado>) -> Resultado<Ficha> {
    estado.con_mut(|d| {
        d.restablecer();
        ficha_de(d)
    })
}

#[tauri::command]
pub fn deshacer(estado: tauri::State<'_, Estado>) -> Resultado<Ficha> {
    estado.con_mut(|d| {
        d.deshacer();
        ficha_de(d)
    })
}

#[tauri::command]
pub fn rehacer(estado: tauri::State<'_, Estado>) -> Resultado<Ficha> {
    estado.con_mut(|d| {
        d.rehacer();
        ficha_de(d)
    })
}

/// El histograma.
///
/// `completo` a `false` da el rápido, calculado sobre una versión reducida:
/// es el que se pide mientras se arrastra un deslizador. A `true` recorre la
/// foto entera y es el que vale para el aviso de recorte. El porqué de la
/// diferencia está en `pf_document::Documento::histograma_rapido`.
#[tauri::command]
pub fn histograma(estado: tauri::State<'_, Estado>, completo: bool) -> Resultado<Histograma> {
    estado.con(|d| if completo { d.histograma() } else { d.histograma_rapido(1024) })
}

#[tauri::command]
pub fn exportar(
    estado: tauri::State<'_, Estado>,
    ruta: String,
    formato: pf_image::exportar::Formato,
) -> Resultado<String> {
    let destino = PathBuf::from(&ruta);

    // La comprobación que evita el peor fallo posible de este programa: que
    // exportar machaque la foto de la que se partió. Pasa con un solo despiste
    // en el diálogo de guardar, y lo que se pierde no se recupera.
    let mismo_archivo = estado.con(|d| {
        // `canonicalize` falla si el destino todavía no existe, que es el caso
        // normal; entonces basta con comparar las rutas tal cual.
        match (d.ruta().canonicalize(), destino.canonicalize()) {
            (Ok(a), Ok(b)) => a == b,
            _ => d.ruta() == destino,
        }
    })?;
    if mismo_archivo {
        return Err(Error::Escritura {
            ruta,
            causa: String::from(
                "ese es el archivo original. PhotoForge no lo sobrescribe nunca: elige otro nombre",
            ),
        });
    }

    let imagen = estado.con(|d| d.completa(true))?;
    pf_image::exportar::exportar(&imagen, &destino, formato)?;
    Ok(destino.display().to_string())
}
