//! Datos de imagen de PhotoForge.
//!
//! # La decisión que manda sobre todas las demás
//!
//! Dentro de PhotoForge una imagen es **RGB lineal en `f32`**, y nunca otra
//! cosa. No hay un `Imagen8Bits` ni un `ImagenPaleta`: hay un solo tipo, y
//! cargar un JPEG consiste en convertirlo a ese tipo.
//!
//! Por qué lineal: los valores de un JPEG no son luz, son luz pasada por la
//! curva de sRGB. Si se sube la exposición multiplicando esos valores, no se
//! está duplicando la luz, se está duplicando un número que representa la luz
//! de forma torcida; el resultado tiene los medios tonos lavados y las mezclas
//! de color sucias. En lineal, «un paso más de exposición» es literalmente
//! multiplicar por dos, y un desenfoque promedia fotones en vez de promediar
//! códigos de archivo.
//!
//! Por qué `f32` y no `u16`: un editor no destructivo encadena operaciones, y
//! con enteros cada una redondea. Además la exposición puede sacar valores por
//! encima de 1,0 y una corrección posterior puede devolverlos al rango; con
//! `u16` ese brillo se habría recortado y perdido para siempre. El precio es
//! la memoria: una foto de 50 Mpx son 600 MB en vez de 150. Se paga.
//!
//! Por qué tres canales y no cuatro: PhotoForge edita fotografías, y una
//! fotografía no tiene transparencia. Reservar un canal alfa que siempre vale
//! 1 cuesta un tercio más de memoria —200 MB en esa foto de 50 Mpx— para nada.
//! Las capas con máscara llevan su máscara aparte.

pub mod carga;
pub mod color;
pub mod exportar;
pub mod histograma;
pub mod metadatos;
pub mod muestreo;
pub mod orientacion;
pub mod raw;

use pf_core::{Dimensiones, Error, Resultado};

/// Canales por píxel. Está aquí como constante y no como `3` suelto porque
/// aparece en todos los índices y un `3` perdido no se puede buscar.
pub const CANALES: usize = 3;

/// Una imagen en RGB lineal.
#[derive(Clone, PartialEq)]
pub struct Imagen {
    dim: Dimensiones,
    /// `dim.pixeles() * CANALES` valores, en orden R, G, B por píxel y los
    /// píxeles por filas de arriba abajo.
    datos: Vec<f32>,
}

impl Imagen {
    /// Construye una imagen a partir de sus valores.
    ///
    /// Por qué devuelve `Resultado` y no toma los datos a ciegas: este es el
    /// único sitio por el que entran píxeles al sistema. Si la longitud del
    /// vector no cuadra con las dimensiones, todo el código de detrás indexa
    /// fuera de rango, y vale mucho más un error aquí que un pánico allí.
    pub fn nueva(dim: Dimensiones, datos: Vec<f32>) -> Resultado<Self> {
        let esperados = dim.pixeles() * CANALES;
        if datos.len() != esperados {
            return Err(Error::Lectura {
                ruta: String::from("(en memoria)"),
                causa: format!(
                    "una imagen de {dim} necesita {esperados} valores y se recibieron {}",
                    datos.len()
                ),
            });
        }
        Ok(Self { dim, datos })
    }

    /// Una imagen negra del tamaño pedido.
    pub fn negra(dim: Dimensiones) -> Self {
        Self { dim, datos: vec![0.0; dim.pixeles() * CANALES] }
    }

    pub fn dim(&self) -> Dimensiones {
        self.dim
    }

    pub fn ancho(&self) -> u32 {
        self.dim.ancho
    }

    pub fn alto(&self) -> u32 {
        self.dim.alto
    }

    pub fn datos(&self) -> &[f32] {
        &self.datos
    }

    pub fn datos_mut(&mut self) -> &mut [f32] {
        &mut self.datos
    }

    /// Cuánta memoria ocupan los píxeles, en bytes. Para poder decirlo en la
    /// interfaz en vez de que el usuario lo descubra cuando el equipo nada.
    pub fn bytes(&self) -> usize {
        self.datos.len() * std::mem::size_of::<f32>()
    }

    /// Convierte a RGB de 8 bits con la curva de sRGB puesta, que es lo que
    /// espera un lienzo de HTML y cualquier visor.
    pub fn a_rgb8(&self) -> Vec<u8> {
        use rayon::prelude::*;
        let tabla = color::tabla_salida();
        let mut destino = vec![0u8; self.datos.len()];
        destino
            .par_chunks_mut(4096)
            .zip(self.datos.par_chunks(4096))
            .for_each(|(salida, entrada)| {
                for (s, e) in salida.iter_mut().zip(entrada) {
                    *s = tabla.a_byte(*e);
                }
            });
        destino
    }

    /// Lo mismo, pero con un canal alfa opaco: `putImageData` del lienzo pide
    /// RGBA y no acepta RGB.
    pub fn a_rgba8(&self) -> Vec<u8> {
        use rayon::prelude::*;
        let tabla = color::tabla_salida();
        let mut destino = vec![255u8; self.dim.pixeles() * 4];
        destino
            .par_chunks_mut(4)
            .zip(self.datos.par_chunks(CANALES))
            .for_each(|(salida, px)| {
                salida[0] = tabla.a_byte(px[0]);
                salida[1] = tabla.a_byte(px[1]);
                salida[2] = tabla.a_byte(px[2]);
            });
        destino
    }
}

/// Por qué un `Debug` a mano: el que genera el compilador imprimiría los
/// ciento cincuenta millones de valores de una foto de 50 Mpx. Basta con
/// depurar una vez con `dbg!` para entender por qué esto no es un capricho.
impl std::fmt::Debug for Imagen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Imagen({}, {} MB)", self.dim, self.bytes() / 1_048_576)
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn no_se_puede_construir_una_imagen_con_los_datos_descuadrados() {
        let dim = Dimensiones::nueva(2, 2).unwrap();
        assert!(Imagen::nueva(dim, vec![0.0; 12]).is_ok());
        assert!(Imagen::nueva(dim, vec![0.0; 11]).is_err());
        assert!(Imagen::nueva(dim, vec![0.0; 13]).is_err());
    }

    #[test]
    fn el_blanco_lineal_sale_blanco_en_8_bits() {
        let dim = Dimensiones::nueva(1, 1).unwrap();
        let img = Imagen::nueva(dim, vec![1.0, 1.0, 1.0]).unwrap();
        assert_eq!(img.a_rgb8(), vec![255, 255, 255]);
        assert_eq!(img.a_rgba8(), vec![255, 255, 255, 255]);
    }

    #[test]
    fn el_gris_medio_lineal_sale_cerca_de_118_y_no_de_46() {
        // 0,18 en luz lineal es el gris medio fotográfico. Convertido con la
        // curva de sRGB da ~118 de 255. Si alguien se saltara la conversión y
        // escribiera 0,18 × 255 directamente saldría 46: casi negro. Esta
        // prueba es la que avisa de que la curva desapareció.
        let dim = Dimensiones::nueva(1, 1).unwrap();
        let img = Imagen::nueva(dim, vec![0.18; 3]).unwrap();
        let v = img.a_rgb8()[0];
        assert!((117..=119).contains(&v), "gris medio salió en {v}");
    }
}
