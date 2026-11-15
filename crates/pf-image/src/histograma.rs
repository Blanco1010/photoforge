//! El histograma que se enseña en el panel.

use crate::{CANALES, Imagen, color};

/// Cuántas columnas tiene el histograma.
///
/// 256 y no otra cosa porque el histograma se mira para decidir si algo se
/// está quemando o empastando, y eso se juzga contra los 256 niveles que va a
/// tener el archivo exportado. Un histograma de 1024 columnas enseñaría
/// detalle que el archivo final no va a poder guardar.
pub const COLUMNAS: usize = 256;

#[derive(Debug, Clone, serde::Serialize)]
pub struct Histograma {
    pub rojo: Vec<u32>,
    pub verde: Vec<u32>,
    pub azul: Vec<u32>,
    pub luminancia: Vec<u32>,
    /// El valor más alto de las cuatro series, para escalar el dibujo sin que
    /// el frontend tenga que recorrerlas otra vez.
    pub maximo: u32,
    /// Píxeles que se salen por abajo y por arriba. Son el dato por el que se
    /// mira un histograma de verdad: no la forma, sino si hay algo pegado a
    /// los extremos.
    pub recortados_negros: u32,
    pub recortados_blancos: u32,
    /// Sobre cuántos píxeles se calculó. Se enseña porque el histograma es una
    /// muestra, no un censo, y decirlo es más honrado que dejarlo implícito.
    pub muestras: u32,
}

/// Tope de píxeles que se miran.
///
/// Por qué se muestrea en vez de contarlos todos: el histograma se recalcula
/// con cada movimiento del deslizador, y recorrer 50 millones de píxeles cada
/// vez es trabajo tirado. Con un millón de muestras repartidas por toda la
/// imagen la forma de la curva es indistinguible de la real —el error de un
/// muestreo así está en la tercera cifra— y el recálculo baja de cientos de
/// milisegundos a unidades.
const MAXIMO_MUESTRAS: usize = 1_000_000;

/// Calcula el histograma de una imagen.
///
/// Por qué sobre los valores con la curva de sRGB puesta y no sobre los
/// lineales: el histograma se lee mirando dónde están las sombras, los medios
/// y las luces, y esas tres zonas son perceptuales. Un histograma lineal
/// amontona casi todo contra el borde izquierdo y no se puede interpretar,
/// aunque sea «más correcto». Esto es lo que hace cualquier editor, y por eso
/// la forma que sale aquí es comparable con la de Lightroom o Darktable.
pub fn calcular(img: &Imagen) -> Histograma {
    let total = img.dim().pixeles();
    // El salto se calcula en píxeles enteros para que el muestreo recorra la
    // imagen de punta a punta en vez de quedarse en la primera parte.
    let salto = (total / MAXIMO_MUESTRAS).max(1);

    let mut rojo = vec![0u32; COLUMNAS];
    let mut verde = vec![0u32; COLUMNAS];
    let mut azul = vec![0u32; COLUMNAS];
    let mut luminancia = vec![0u32; COLUMNAS];
    let mut recortados_negros = 0u32;
    let mut recortados_blancos = 0u32;
    let mut muestras = 0u32;

    let tabla = color::tabla_salida();
    let datos = img.datos();

    for p in (0..total).step_by(salto) {
        let i = p * CANALES;
        let (lr, lg, lb) = (datos[i], datos[i + 1], datos[i + 2]);

        // Los coeficientes de luminancia de Rec. 709, que son los de sRGB.
        // Se aplican sobre los valores LINEALES —que es donde tienen sentido
        // físico— y la curva se pone después.
        let lum = 0.2126 * lr + 0.7152 * lg + 0.0722 * lb;

        let (r, g, b, l) = (
            tabla.a_byte(lr) as usize,
            tabla.a_byte(lg) as usize,
            tabla.a_byte(lb) as usize,
            tabla.a_byte(lum) as usize,
        );
        rojo[r] += 1;
        verde[g] += 1;
        azul[b] += 1;
        luminancia[l] += 1;

        // Recortado es que CUALQUIERA de los tres canales se haya ido, no que
        // se hayan ido los tres: un rojo quemado ya es detalle perdido aunque
        // el verde y el azul estén dentro.
        if lr <= 0.0 || lg <= 0.0 || lb <= 0.0 {
            recortados_negros += 1;
        }
        if lr >= 1.0 || lg >= 1.0 || lb >= 1.0 {
            recortados_blancos += 1;
        }
        muestras += 1;
    }

    // El máximo se busca saltándose las dos columnas de los extremos. Una foto
    // con cielo quemado tiene decenas de miles de píxeles en la columna 255, y
    // escalar el dibujo con ese pico deja el resto del histograma aplastado
    // contra el suelo, que es justo lo que no se quiere ver.
    let maximo = [&rojo, &verde, &azul, &luminancia]
        .iter()
        .flat_map(|serie| serie[1..COLUMNAS - 1].iter())
        .copied()
        .max()
        .unwrap_or(1)
        .max(1);

    Histograma {
        rojo,
        verde,
        azul,
        luminancia,
        maximo,
        recortados_negros,
        recortados_blancos,
        muestras,
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use pf_core::Dimensiones;

    fn imagen_de(valores: &[[f32; 3]]) -> Imagen {
        let dim = Dimensiones::nueva(valores.len() as u32, 1).unwrap();
        Imagen::nueva(dim, valores.iter().flatten().copied().collect()).unwrap()
    }

    #[test]
    fn una_imagen_gris_cae_entera_en_una_columna() {
        let img = imagen_de(&[[0.18; 3]; 100]);
        let h = calcular(&img);
        assert_eq!(h.rojo[118], 100);
        assert_eq!(h.muestras, 100);
        assert_eq!(h.recortados_blancos, 0);
        assert_eq!(h.recortados_negros, 0);
    }

    #[test]
    fn el_recorte_se_cuenta_aunque_solo_se_vaya_un_canal() {
        // Rojo quemado, verde y azul dentro. Es un píxel recortado.
        let img = imagen_de(&[[1.5, 0.3, 0.3]]);
        let h = calcular(&img);
        assert_eq!(h.recortados_blancos, 1);
        assert_eq!(h.recortados_negros, 0);
    }

    #[test]
    fn el_maximo_ignora_los_extremos_para_no_aplastar_el_dibujo() {
        // 1000 píxeles blancos quemados y 10 grises. Si el máximo saliera
        // 1000, los grises no se verían. Tiene que salir 10.
        let mut v = vec![[2.0f32; 3]; 1000];
        v.extend(vec![[0.18f32; 3]; 10]);
        let h = calcular(&imagen_de(&v));
        assert_eq!(h.rojo[255], 1000);
        assert_eq!(h.maximo, 10);
    }

    #[test]
    fn el_muestreo_recorre_la_imagen_entera_y_no_solo_el_principio() {
        // Mitad negra y mitad blanca, con más píxeles que el tope de muestras.
        // Si el muestreo cogiera el primer millón seguido, saldría todo negro.
        let mitad = MAXIMO_MUESTRAS;
        let mut v = vec![[0.0f32; 3]; mitad];
        v.extend(vec![[1.0f32; 3]; mitad]);
        let h = calcular(&imagen_de(&v));
        let negros = h.luminancia[0];
        let blancos = h.luminancia[255];
        assert!(negros > 0 && blancos > 0, "negros {negros}, blancos {blancos}");
        let desvio = (negros as i64 - blancos as i64).abs();
        assert!(desvio <= 2, "el muestreo salió descompensado: {negros} vs {blancos}");
    }
}
