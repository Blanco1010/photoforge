//! Poner la foto derecha.
//!
//! # Por qué esto no es un detalle
//!
//! Una cámara de móvil no gira los píxeles cuando giras el teléfono: los
//! guarda siempre en el mismo sentido y apunta en los metadatos cómo hay que
//! girarlos para verlos. Si un programa ignora ese dato, la mitad de las
//! fotos salen tumbadas.
//!
//! Aquí se aplica **al abrir**, y a partir de ahí el resto de PhotoForge
//! trabaja con una imagen que ya está derecha. La alternativa —arrastrar la
//! orientación por todo el programa y girar solo al pintar— obliga a que el
//! lienzo, el histograma, el recorte, la exportación y cualquier operación
//! futura se acuerden de ella. Uno se olvidará.

use pf_core::{Dimensiones, Resultado};
use rayon::prelude::*;

use crate::{CANALES, Imagen};

/// Cómo hay que girar la foto, con los mismos números que usa el EXIF.
///
/// Los valores son los de la etiqueta 0x0112 del TIFF/EXIF, del 1 al 8. Se
/// guardan así, y no como un ángulo, porque cuatro de los ocho casos incluyen
/// un espejo y no se pueden expresar con un giro.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Orientacion(u8);

impl Orientacion {
    pub const DERECHA: Orientacion = Orientacion(1);

    /// Del número del EXIF. Cualquier cosa fuera de 1..=8 se trata como
    /// «derecha»: un archivo con la etiqueta rota es muchísimo más probable
    /// que una foto que de verdad había que girar de una forma imposible.
    pub fn desde_exif(v: u16) -> Self {
        if (1..=8).contains(&v) { Orientacion(v as u8) } else { Orientacion(1) }
    }

    pub fn numero(&self) -> u8 {
        self.0
    }

    pub fn no_hay_que_hacer_nada(&self) -> bool {
        self.0 == 1
    }

    /// Las tres operaciones elementales en las que se descompone.
    ///
    /// `(trasponer, espejo_horizontal, espejo_vertical)`. El orden de
    /// aplicación importa: primero los espejos y después la trasposición.
    fn pasos(&self) -> (bool, bool, bool) {
        match self.0 {
            2 => (false, true, false),  // espejo horizontal
            3 => (false, true, true),   // 180°
            4 => (false, false, true),  // espejo vertical
            5 => (true, false, false),  // traspuesta
            6 => (true, false, true),   // 90° a la derecha
            7 => (true, true, true),    // transversa
            8 => (true, true, false),   // 90° a la izquierda
            _ => (false, false, false), // 1 y cualquier cosa rara
        }
    }

    /// Qué va a medir la foto después de enderezarla.
    pub fn dimensiones_resultantes(&self, d: Dimensiones) -> Dimensiones {
        let (trasponer, _, _) = self.pasos();
        if trasponer {
            Dimensiones { ancho: d.alto, alto: d.ancho }
        } else {
            d
        }
    }
}

/// Describe en castellano lo que hace una orientación, para la ficha.
pub fn describir(o: Orientacion) -> Option<&'static str> {
    match o.numero() {
        2 => Some("espejo horizontal"),
        3 => Some("girada 180°"),
        4 => Some("espejo vertical"),
        5 => Some("traspuesta"),
        6 => Some("girada 90° a la derecha"),
        7 => Some("transversa"),
        8 => Some("girada 90° a la izquierda"),
        _ => None,
    }
}

/// Devuelve la imagen enderezada.
///
/// Si no hay nada que hacer devuelve la misma imagen sin copiarla: es el caso
/// de la mayoría de los archivos, y una foto de 50 Mpx son 600 MB que no vale
/// la pena duplicar para no hacer nada.
///
/// Se recorre el DESTINO y no el origen —para cada píxel de salida se busca
/// de dónde viene— porque así cada hilo escribe en una zona suya y rayon
/// puede repartir las filas sin que dos hilos se pisen.
pub fn enderezar(img: Imagen, o: Orientacion) -> Resultado<Imagen> {
    if o.no_hay_que_hacer_nada() {
        return Ok(img);
    }
    let (trasponer, espejo_h, espejo_v) = o.pasos();
    let origen = img.dim();
    let destino = o.dimensiones_resultantes(origen);

    let (ancho_o, alto_o) = (origen.ancho as usize, origen.alto as usize);
    let ancho_d = destino.ancho as usize;
    let fuente = img.datos();
    let mut datos = vec![0.0f32; destino.pixeles() * CANALES];

    datos
        .par_chunks_mut(ancho_d * CANALES)
        .enumerate()
        .for_each(|(dy, fila)| {
            for dx in 0..ancho_d {
                // Deshacer la trasposición y después los espejos: es el
                // camino de vuelta del que describe `pasos`.
                let (mut x, mut y) = if trasponer { (dy, dx) } else { (dx, dy) };
                if espejo_h {
                    x = ancho_o - 1 - x;
                }
                if espejo_v {
                    y = alto_o - 1 - y;
                }
                let o = (y * ancho_o + x) * CANALES;
                let d = dx * CANALES;
                fila[d] = fuente[o];
                fila[d + 1] = fuente[o + 1];
                fila[d + 2] = fuente[o + 2];
            }
        });

    Imagen::nueva(destino, datos)
}

#[cfg(test)]
mod pruebas {
    use super::*;

    /// Una imagen de 3×2 con los valores 0..5, para poder seguir cada píxel.
    ///
    /// ```text
    /// 0 1 2
    /// 3 4 5
    /// ```
    fn numerada() -> Imagen {
        let dim = Dimensiones::nueva(3, 2).unwrap();
        let datos: Vec<f32> = (0..6).flat_map(|v| [v as f32; CANALES]).collect();
        Imagen::nueva(dim, datos).unwrap()
    }

    fn valores(img: &Imagen) -> Vec<u32> {
        img.datos().chunks(CANALES).map(|c| c[0] as u32).collect()
    }

    #[test]
    fn la_orientacion_normal_no_copia_ni_cambia_nada() {
        let img = numerada();
        let salida = enderezar(img.clone(), Orientacion::DERECHA).unwrap();
        assert_eq!(salida.datos(), img.datos());
        assert_eq!(salida.dim(), img.dim());
    }

    #[test]
    fn el_giro_de_90_a_la_derecha_deja_la_foto_de_pie() {
        // 0 1 2        3 0
        // 3 4 5   ->   4 1
        //              5 2
        let salida = enderezar(numerada(), Orientacion::desde_exif(6)).unwrap();
        assert_eq!(salida.dim(), Dimensiones::nueva(2, 3).unwrap());
        assert_eq!(valores(&salida), vec![3, 0, 4, 1, 5, 2]);
    }

    #[test]
    fn el_giro_de_90_a_la_izquierda_es_el_contrario() {
        // 0 1 2        2 5
        // 3 4 5   ->   1 4
        //              0 3
        let salida = enderezar(numerada(), Orientacion::desde_exif(8)).unwrap();
        assert_eq!(salida.dim(), Dimensiones::nueva(2, 3).unwrap());
        assert_eq!(valores(&salida), vec![2, 5, 1, 4, 0, 3]);
    }

    #[test]
    fn los_espejos_y_el_giro_de_180_no_cambian_el_tamano() {
        assert_eq!(valores(&enderezar(numerada(), Orientacion::desde_exif(2)).unwrap()),
                   vec![2, 1, 0, 5, 4, 3]);
        assert_eq!(valores(&enderezar(numerada(), Orientacion::desde_exif(4)).unwrap()),
                   vec![3, 4, 5, 0, 1, 2]);
        assert_eq!(valores(&enderezar(numerada(), Orientacion::desde_exif(3)).unwrap()),
                   vec![5, 4, 3, 2, 1, 0]);
    }

    #[test]
    fn girar_cuatro_veces_noventa_grados_devuelve_la_foto_de_partida() {
        // La prueba que coge un espejo colado de más: cualquier error de
        // simetría sobrevive a un giro, pero no a los cuatro.
        let mut img = numerada();
        for _ in 0..4 {
            img = enderezar(img, Orientacion::desde_exif(6)).unwrap();
        }
        assert_eq!(valores(&img), valores(&numerada()));
        assert_eq!(img.dim(), numerada().dim());
    }

    #[test]
    fn un_numero_de_orientacion_imposible_se_trata_como_derecha() {
        for v in [0u16, 9, 255, 65535] {
            assert!(Orientacion::desde_exif(v).no_hay_que_hacer_nada(), "el {v} no se ignoró");
        }
    }
}
