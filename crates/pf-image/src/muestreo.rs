//! Sacar de una imagen grande el trozo que cabe en la pantalla.
//!
//! # Por qué esto es el corazón del rendimiento
//!
//! Una foto de 50 Mpx no cabe en un lienzo de 1.600 × 900, que son 1,4 Mpx. Si
//! cada movimiento del deslizador de exposición recalculara los 50 millones de
//! píxeles para enseñar 1,4 millones, se estaría tirando el 97 % del trabajo.
//!
//! Así que el orden es al revés del evidente: primero se decide QUÉ trozo se
//! ve y a qué resolución, se saca solo eso, y las operaciones de revelado se
//! aplican encima del resultado. El coste deja de depender del tamaño de la
//! foto y pasa a depender del tamaño de la ventana, que es constante.
//!
//! Esto tiene una consecuencia que hay que decir en voz alta: lo que se ve en
//! pantalla es una aproximación de lo que saldrá al exportar, porque está
//! calculado sobre menos píxeles. Para exposición y contraste, que tratan cada
//! píxel por separado, la aproximación es exacta. Para lo que venga después
//! —enfoque, reducción de ruido, cualquier cosa que mire a los vecinos— no lo
//! será, y habrá que decirlo en la interfaz.

use pf_core::{Dimensiones, Region};

use crate::orientacion::{self, Orientacion};
use rayon::prelude::*;

use crate::{CANALES, Imagen};

/// Saca de `origen` la región pedida, reescalada al tamaño de `destino`.
///
/// La región va en coordenadas de imagen con decimales y puede caer
/// parcialmente fuera: lo que quede fuera se repite desde el borde. Repetir el
/// borde y no pintar negro es a propósito —un borde negro falso al alejar la
/// vista parece parte de la foto—, pero la interfaz debería pedir solo la
/// intersección con la imagen y no llegar a depender de esto.
pub fn muestrear(origen: &Imagen, region: Region, destino: Dimensiones) -> Imagen {
    muestrear_girado(origen, region, destino, Orientacion::DERECHA)
}

/// Lo mismo, pero sobre la foto GIRADA sin llegar a girarla.
///
/// `region` y `destino` van en coordenadas de la foto ya enderezada, que es
/// la que el usuario está viendo. Por dentro se muestrea sobre esa rejilla
/// imaginaria y cada índice se traduce al original con
/// [`orientacion::indice_en_origen`].
///
/// Por qué así y no girando el trozo después de muestrearlo: porque un giro
/// es una permutación, traducirlo a nivel de índice es exacto, y girar el
/// trozo costaría reservar y rellenar de ceros 17 MB en cada cuadro para
/// luego sobrescribirlos. El porqué largo está en `indice_en_origen`.
pub fn muestrear_girado(
    origen: &Imagen,
    region: Region,
    destino: Dimensiones,
    giro: Orientacion,
) -> Imagen {
    let escala_x = region.ancho / destino.ancho as f32;
    let escala_y = region.alto / destino.alto as f32;

    // La frontera entre los dos algoritmos.
    //
    // Al ampliar (escala < 1) se usa el vecino más cercano: en un editor de
    // fotografía, ampliar es ir a mirar los píxeles de verdad, y una
    // interpolación suave enseñaría un detalle que no existe. Al alejar
    // (escala > 1) hace falta promediar, porque coger un píxel de cada cuatro
    // convierte cualquier textura fina en un moiré que no está en la foto.
    let hay_que_promediar = escala_x > 1.0 || escala_y > 1.0;

    let mut datos = vec![0.0f32; destino.pixeles() * CANALES];
    let ancho_destino = destino.ancho as usize;
    let fuente = origen.datos();
    let (ancho_origen, alto_origen) = (origen.ancho() as i64, origen.alto() as i64);
    // Las medidas de la foto YA girada: es la rejilla sobre la que se
    // muestrea, aunque esos píxeles no existan en ningún sitio.
    let girada = giro.dimensiones_resultantes(origen.dim());
    let (ancho_vista, alto_vista) = (girada.ancho as i64, girada.alto as i64);
    let pasos = giro.pasos_publicos();

    datos
        .par_chunks_mut(ancho_destino * CANALES)
        .enumerate()
        .for_each(|(dy, fila)| {
            let iy = region.y + (dy as f32 + 0.5) * escala_y;
            for dx in 0..ancho_destino {
                let ix = region.x + (dx as f32 + 0.5) * escala_x;
                let leer = |x: i64, y: i64| -> usize {
                    // Primero se acota en la rejilla GIRADA, que es la que
                    // el usuario ve, y después se traduce al original: al
                    // revés, repetir el borde repetiría el borde equivocado.
                    let x = x.clamp(0, ancho_vista - 1);
                    let y = y.clamp(0, alto_vista - 1);
                    let (ox, oy) =
                        orientacion::indice_en_origen(pasos, ancho_origen, alto_origen, x, y);
                    (oy as usize * ancho_origen as usize + ox as usize) * CANALES
                };
                let px = if hay_que_promediar {
                    promedio(
                        fuente,
                        &leer,
                        ix - escala_x * 0.5,
                        iy - escala_y * 0.5,
                        escala_x,
                        escala_y,
                        ancho_vista,
                        alto_vista,
                    )
                } else {
                    cercano(fuente, &leer, ix, iy)
                };
                let d = dx * CANALES;
                fila[d] = px[0];
                fila[d + 1] = px[1];
                fila[d + 2] = px[2];
            }
        });

    // `nueva` no puede fallar: el vector se acaba de dimensionar aquí mismo.
    Imagen::nueva(destino, datos).expect("el muestreo dimensionó mal su propio vector")
}

#[inline]
fn cercano(fuente: &[f32], leer: &impl Fn(i64, i64) -> usize, x: f32, y: f32) -> [f32; CANALES] {
    let i = leer(x.floor() as i64, y.floor() as i64);
    [fuente[i], fuente[i + 1], fuente[i + 2]]
}

/// Media de todos los píxeles de origen que caen bajo un píxel de destino.
///
/// Por qué la media y no un filtro con pesos (Lanczos y compañía): la media
/// de caja es exactamente lo que hace un sensor al juntar píxeles, no inventa
/// contraste en los bordes y cuesta una suma por píxel. Los filtros con pesos
/// dan una miniatura más nítida, y ese es su problema: una miniatura más
/// nítida que la foto miente sobre el enfoque, que es justo lo que alguien
/// está comprobando cuando aleja la vista.
#[inline]
#[allow(clippy::too_many_arguments)]
fn promedio(
    fuente: &[f32],
    leer: &impl Fn(i64, i64) -> usize,
    x0: f32,
    y0: f32,
    paso_x: f32,
    paso_y: f32,
    ancho: i64,
    alto: i64,
) -> [f32; CANALES] {
    let xi = (x0.floor() as i64).max(0);
    let yi = (y0.floor() as i64).max(0);
    let xf = ((x0 + paso_x).ceil() as i64).min(ancho).max(xi + 1);
    let yf = ((y0 + paso_y).ceil() as i64).min(alto).max(yi + 1);

    let mut acc = [0.0f32; CANALES];
    let mut n = 0u32;
    for y in yi..yf {
        for x in xi..xf {
            let i = leer(x, y);
            acc[0] += fuente[i];
            acc[1] += fuente[i + 1];
            acc[2] += fuente[i + 2];
            n += 1;
        }
    }
    let n = n.max(1) as f32;
    [acc[0] / n, acc[1] / n, acc[2] / n]
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn degradado(ancho: u32, alto: u32) -> Imagen {
        let dim = Dimensiones::nueva(ancho, alto).unwrap();
        let mut datos = Vec::with_capacity(dim.pixeles() * CANALES);
        for y in 0..alto {
            for x in 0..ancho {
                let v = (x + y * ancho) as f32;
                datos.extend_from_slice(&[v, v, v]);
            }
        }
        Imagen::nueva(dim, datos).unwrap()
    }

    #[test]
    fn muestrear_la_imagen_entera_a_su_propio_tamano_la_deja_igual() {
        // La prueba de que no hay un desplazamiento de medio píxel escondido.
        let img = degradado(8, 8);
        let r = Region { x: 0.0, y: 0.0, ancho: 8.0, alto: 8.0 };
        let salida = muestrear(&img, r, img.dim());
        assert_eq!(salida.datos(), img.datos());
    }

    #[test]
    fn al_alejar_se_promedia_en_vez_de_coger_uno_de_cada_cuatro() {
        // Una imagen de 2x2 con valores 0, 1, 2, 3 reducida a 1x1 tiene que
        // dar 1,5. Si diera 0 estaría cogiendo el vecino más cercano, que es
        // el error que crea moiré.
        let dim = Dimensiones::nueva(2, 2).unwrap();
        let img = Imagen::nueva(dim, vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 2.0, 2.0, 2.0, 3.0, 3.0, 3.0]).unwrap();
        let r = Region { x: 0.0, y: 0.0, ancho: 2.0, alto: 2.0 };
        let salida = muestrear(&img, r, Dimensiones::nueva(1, 1).unwrap());
        assert_eq!(salida.datos(), &[1.5, 1.5, 1.5]);
    }

    #[test]
    fn al_ampliar_se_repiten_pixeles_enteros_y_no_se_inventa_nada() {
        // 2x1 con valores 0 y 10 ampliado a 4x1: tiene que salir 0,0,10,10.
        // Un 2,5 o un 5 por el medio sería interpolación.
        let dim = Dimensiones::nueva(2, 1).unwrap();
        let img = Imagen::nueva(dim, vec![0.0, 0.0, 0.0, 10.0, 10.0, 10.0]).unwrap();
        let r = Region { x: 0.0, y: 0.0, ancho: 2.0, alto: 1.0 };
        let salida = muestrear(&img, r, Dimensiones::nueva(4, 1).unwrap());
        let v: Vec<f32> = salida.datos().chunks(3).map(|c| c[0]).collect();
        assert_eq!(v, vec![0.0, 0.0, 10.0, 10.0]);
    }

    #[test]
    fn una_region_que_se_sale_repite_el_borde_en_vez_de_indexar_fuera() {
        // Lo que esta prueba vigila de verdad es que no haya un pánico.
        let img = degradado(4, 4);
        let r = Region { x: -10.0, y: -10.0, ancho: 30.0, alto: 30.0 };
        let salida = muestrear(&img, r, Dimensiones::nueva(16, 16).unwrap());
        assert_eq!(salida.dim().pixeles(), 256);
    }
}
