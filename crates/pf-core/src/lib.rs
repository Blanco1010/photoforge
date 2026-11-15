//! Vocabulario común de PhotoForge.
//!
//! Aquí solo vive lo que TODAS las demás cajas necesitan: el error y un par de
//! tipos geométricos. La tentación de meter aquí «lo que no sabe dónde ir» es
//! lo que convierte una caja `core` en un vertedero, así que la regla es
//! estricta: si solo lo usa una caja, va en esa caja.

use std::fmt;

/// El error de PhotoForge.
///
/// Por qué un enum propio y no `anyhow`: este error cruza la frontera a la
/// interfaz y se le enseña al usuario. `anyhow` es cómodo para una herramienta
/// de línea de comandos, donde el mensaje acaba en una terminal y lo lee un
/// programador; aquí el mensaje acaba en una ventana y lo lee alguien que
/// quería abrir una foto. Un enum obliga a redactar cada caso.
///
/// Por qué las variantes llevan `String` y no el error original: `pf-core` no
/// depende de `image`, ni de `exif`, ni de `rawler`. Si guardara sus errores
/// tendría que depender de las tres, y entonces cambiar de decodificador
/// obligaría a tocar la caja que todas comparten. El precio es perder la
/// cadena de causas; a cambio, el núcleo no sabe con qué se leen los archivos.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("no se pudo leer «{ruta}»: {causa}")]
    Lectura { ruta: String, causa: String },

    #[error("no se pudo escribir «{ruta}»: {causa}")]
    Escritura { ruta: String, causa: String },

    #[error("«{0}» no es un formato que PhotoForge sepa abrir")]
    FormatoNoSoportado(String),

    #[error("la imagen dice medir {ancho}×{alto}, y eso no puede ser")]
    DimensionesInvalidas { ancho: u32, alto: u32 },

    #[error("no hay ninguna foto abierta")]
    SinDocumento,
}

/// Resultado con el error de la casa ya puesto.
pub type Resultado<T> = std::result::Result<T, Error>;

/// Por qué se serializa a texto plano y no como objeto: al otro lado hay
/// TypeScript, y lo único que hace con el error es enseñarlo. Un objeto con
/// la variante y sus campos obligaría a mantener el mismo enum en dos
/// lenguajes para no ganar nada.
impl serde::Serialize for Error {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

/// Tamaño en píxeles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Dimensiones {
    pub ancho: u32,
    pub alto: u32,
}

impl Dimensiones {
    pub fn nueva(ancho: u32, alto: u32) -> Resultado<Self> {
        // Por qué se comprueba aquí y no en quien llama: todo el código que
        // viene detrás multiplica ancho por alto por tres para reservar un
        // vector. Un cero convierte ese vector en vacío y el primer acceso en
        // un pánico a cien líneas de distancia del motivo real.
        if ancho == 0 || alto == 0 {
            return Err(Error::DimensionesInvalidas { ancho, alto });
        }
        Ok(Self { ancho, alto })
    }

    /// Número de píxeles. En `usize` porque es lo que piden los índices.
    pub fn pixeles(&self) -> usize {
        self.ancho as usize * self.alto as usize
    }
}

impl fmt::Display for Dimensiones {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}×{}", self.ancho, self.alto)
    }
}

/// De dónde vienen los píxeles de una imagen.
///
/// # Por qué hace falta distinguirlo
///
/// Un JPEG ya está revelado: alguien —la cámara o un programa— decidió su
/// brillo, su contraste y cómo se tratan las luces, y los números que trae
/// describen lo que debe salir por la pantalla. Un RAW no: sus números son
/// cuántos fotones llegaron al sensor, sin interpretar.
///
/// La diferencia no es académica. A un RAW hay que colocarlo en el rango de
/// la pantalla antes de poder mirarlo, y sin hacerlo se ve oscuro aunque esté
/// perfectamente expuesto. A un JPEG hacerle lo mismo sería aplicarle dos
/// veces una interpretación que ya tenía.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Procedencia {
    /// JPEG, PNG, TIFF: ya revelada, lista para la pantalla.
    Revelada,
    /// RAW de cámara: luz medida, todavía sin interpretar.
    Escena,
}

impl Procedencia {
    pub fn es_escena(&self) -> bool {
        matches!(self, Procedencia::Escena)
    }
}

/// Un rectángulo en coordenadas de imagen, en píxeles y con decimales.
///
/// Por qué con decimales y no con enteros: este rectángulo describe el trozo
/// de foto que se ve en pantalla, y al alejar la vista un píxel de pantalla
/// cae entre píxeles de la imagen. Redondear aquí haría que la imagen diera
/// un salto de un píxel cada vez que se mueve el ratón.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Region {
    pub x: f32,
    pub y: f32,
    pub ancho: f32,
    pub alto: f32,
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn dimensiones_en_cero_son_un_error_y_no_un_vector_vacio() {
        assert!(Dimensiones::nueva(0, 100).is_err());
        assert!(Dimensiones::nueva(100, 0).is_err());
        assert!(Dimensiones::nueva(1, 1).is_ok());
    }

    #[test]
    fn el_conteo_de_pixeles_no_se_desborda_en_32_bits() {
        // 60.000 × 60.000 son 3.600 millones de píxeles: más de lo que cabe en
        // un u32. Es una foto imposible hoy, pero el cálculo tiene que seguir
        // dando el número bueno, no dar la vuelta.
        let d = Dimensiones::nueva(60_000, 60_000).unwrap();
        assert_eq!(d.pixeles(), 3_600_000_000);
    }

    #[test]
    fn el_error_se_serializa_como_la_frase_que_vera_el_usuario() {
        let e = Error::SinDocumento;
        let json = serde_json::to_string(&e).unwrap();
        assert_eq!(json, "\"no hay ninguna foto abierta\"");
    }
}
