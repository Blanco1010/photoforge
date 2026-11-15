//! Las operaciones de revelado.

use pf_image::{CANALES, Imagen, color};
use rayon::prelude::*;

/// Una operación de revelado con sus parámetros.
///
/// Por qué un enum y no un trait con objetos: con un trait, guardar el
/// documento obligaría a serializar objetos de tipo desconocido, y leerlo
/// obligaría a un registro de constructores por nombre. Con un enum, serde lo
/// hace solo y el compilador avisa en todos los `match` el día que se añada
/// una operación. Cuando sean treinta operaciones puede que el enum empiece a
/// molestar; con dos, el trait es ceremonia sin beneficio.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "clase", rename_all = "lowercase")]
pub enum Operacion {
    /// Pasos de exposición (EV). Cada paso es el doble de luz.
    Exposicion { pasos: f32 },
    /// Contraste, de -100 a +100. El 0 no toca nada.
    Contraste { cantidad: f32 },
}

/// Nombre corto de la clase de operación, sin sus parámetros.
///
/// Sirve para que «ajustar la exposición» sustituya a la exposición que ya
/// hubiera en vez de añadir una segunda. Es `&'static str` y no otro enum
/// porque un enum paralelo al de arriba habría que mantenerlo sincronizado a
/// mano, y ese es exactamente el error que nadie ve hasta que lo sufre.
pub type Clase = &'static str;

impl Operacion {
    pub fn clase(&self) -> Clase {
        match self {
            Operacion::Exposicion { .. } => "exposicion",
            Operacion::Contraste { .. } => "contraste",
        }
    }

    /// ¿Esta operación deja la imagen como estaba?
    ///
    /// Se usa para no guardar en la lista operaciones que no hacen nada: un
    /// deslizador devuelto a cero no debería dejar rastro en el documento ni
    /// contar como un paso que deshacer.
    pub fn es_neutra(&self) -> bool {
        match self {
            Operacion::Exposicion { pasos } => *pasos == 0.0,
            Operacion::Contraste { cantidad } => *cantidad == 0.0,
        }
    }

    /// Orden canónico de aplicación.
    ///
    /// Por qué existe: el resultado NO puede depender de qué deslizador tocó
    /// el usuario primero. Si la lista se fuera llenando por orden de uso,
    /// subir la exposición y luego el contraste daría una foto distinta que
    /// hacerlo al revés, y el usuario no tendría forma de saber por qué.
    ///
    /// Por qué este orden y no otro: la exposición es una corrección de
    /// captura —cuánta luz había— y el contraste es una decisión de
    /// interpretación. Primero se coloca la foto donde debería haber estado y
    /// después se interpreta, que es el orden en el que trabaja cualquier
    /// revelador.
    pub fn orden(&self) -> u8 {
        match self {
            Operacion::Exposicion { .. } => 10,
            Operacion::Contraste { .. } => 20,
        }
    }
}

/// Aplica la lista de operaciones sobre una imagen, en el sitio.
///
/// Recibe la imagen ya muestreada al tamaño de la vista, no la original: ver
/// `pf_image::muestreo` para el porqué.
pub fn aplicar(operaciones: &[Operacion], img: &mut Imagen) {
    for op in operaciones {
        if op.es_neutra() {
            continue;
        }
        match *op {
            Operacion::Exposicion { pasos } => exposicion(img, pasos),
            Operacion::Contraste { cantidad } => contraste(img, cantidad),
        }
    }
}

/// Exposición: multiplicar la luz.
///
/// Esto es una multiplicación y nada más, y esa sencillez es el argumento
/// entero a favor de trabajar en lineal. Un paso de exposición es el doble de
/// fotones, y en lineal el doble de fotones es el doble de número. En sRGB
/// habría que deshacer la curva, multiplicar y volver a ponerla, tres veces
/// por píxel y por canal.
///
/// No se recorta por arriba a propósito: un valor de 1,8 después de subir la
/// exposición sigue siendo un dato, y bajar el contraste después puede
/// devolverlo al rango visible. El recorte ocurre solo al pintar o al
/// exportar.
fn exposicion(img: &mut Imagen, pasos: f32) {
    let factor = 2.0f32.powf(pasos);
    img.datos_mut().par_chunks_mut(4096).for_each(|trozo| {
        for v in trozo {
            *v *= factor;
        }
    });
}

/// Contraste: separar o juntar los tonos alrededor del gris medio.
///
/// # Por qué el contraste NO se aplica en lineal
///
/// Es la excepción a la regla de la casa, y tiene motivo. En luz lineal, el
/// gris medio está en 0,18 y las sombras se amontonan todas por debajo de
/// 0,05: estirar alrededor de 0,18 mueve muchísimo las luces y casi nada las
/// sombras, así que el primer 20 % del deslizador no parece hacer nada y el
/// último 20 % quema el cielo. El deslizador tiene que sentirse parejo.
///
/// Así que el contraste se aplica sobre el valor perceptual —el mismo que
/// vería la pantalla—, con el pivote en 0,5, y el resultado se devuelve a
/// lineal para que las operaciones siguientes sigan trabajando con luz.
///
/// Las conversiones se extienden fuera de 0..1 sin recortar: un valor de 1,3
/// que entra sale del otro lado, en vez de quedarse pegado al blanco.
fn contraste(img: &mut Imagen, cantidad: f32) {
    let k = 1.0 + cantidad.clamp(-100.0, 100.0) / 100.0;
    img.datos_mut().par_chunks_mut(4096).for_each(|trozo| {
        for v in trozo {
            let s = color::lineal_a_srgb(*v);
            *v = color::srgb_a_lineal((s - 0.5) * k + 0.5);
        }
    });
}

/// Repetido para que `CANALES` siga siendo visible desde este módulo.
#[allow(dead_code)]
const _CANALES: usize = CANALES;

#[cfg(test)]
mod pruebas {
    use super::*;
    use pf_core::Dimensiones;

    fn imagen(valores: &[f32]) -> Imagen {
        let dim = Dimensiones::nueva(valores.len() as u32, 1).unwrap();
        let datos: Vec<f32> = valores.iter().flat_map(|v| [*v, *v, *v]).collect();
        Imagen::nueva(dim, datos).unwrap()
    }

    fn canal_rojo(img: &Imagen) -> Vec<f32> {
        img.datos().chunks(CANALES).map(|c| c[0]).collect()
    }

    #[test]
    fn un_paso_de_exposicion_es_exactamente_el_doble_de_luz() {
        let mut img = imagen(&[0.1, 0.25, 0.5]);
        aplicar(&[Operacion::Exposicion { pasos: 1.0 }], &mut img);
        let v = canal_rojo(&img);
        assert!((v[0] - 0.2).abs() < 1e-6);
        assert!((v[1] - 0.5).abs() < 1e-6);
        assert!((v[2] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn subir_y_bajar_la_exposicion_devuelve_el_valor_original() {
        // La prueba de que no se está recortando por el camino: si el 1,6 de
        // en medio se hubiera pegado a 1,0, al volver saldría 0,5 y no 0,8.
        let mut img = imagen(&[0.8]);
        aplicar(&[Operacion::Exposicion { pasos: 1.0 }], &mut img);
        aplicar(&[Operacion::Exposicion { pasos: -1.0 }], &mut img);
        assert!((canal_rojo(&img)[0] - 0.8).abs() < 1e-6);
    }

    #[test]
    fn el_contraste_no_mueve_el_pivote() {
        // El tono que cae justo en el pivote tiene que quedarse donde está,
        // suba o baje el deslizador. Si se mueve, el pivote está mal puesto.
        let pivote = color::srgb_a_lineal(0.5);
        for cantidad in [-100.0, -40.0, 40.0, 100.0] {
            let mut img = imagen(&[pivote]);
            aplicar(&[Operacion::Contraste { cantidad }], &mut img);
            let v = canal_rojo(&img)[0];
            assert!((v - pivote).abs() < 1e-5, "con {cantidad} el pivote se movió a {v}");
        }
    }

    #[test]
    fn el_contraste_separa_lo_claro_de_lo_oscuro() {
        let oscuro = color::srgb_a_lineal(0.25);
        let claro = color::srgb_a_lineal(0.75);
        let mut img = imagen(&[oscuro, claro]);
        aplicar(&[Operacion::Contraste { cantidad: 50.0 }], &mut img);
        let v = canal_rojo(&img);
        assert!(v[0] < oscuro, "lo oscuro tenía que oscurecerse");
        assert!(v[1] > claro, "lo claro tenía que aclararse");
    }

    #[test]
    fn el_contraste_a_menos_cien_lo_deja_todo_en_el_pivote() {
        let pivote = color::srgb_a_lineal(0.5);
        let mut img = imagen(&[0.01, 0.3, 0.9]);
        aplicar(&[Operacion::Contraste { cantidad: -100.0 }], &mut img);
        for v in canal_rojo(&img) {
            assert!((v - pivote).abs() < 1e-5, "quedó {v} y no el pivote {pivote}");
        }
    }

    #[test]
    fn una_operacion_neutra_no_toca_ni_un_valor() {
        let partida = imagen(&[0.1, 0.5, 0.9]);
        let mut img = partida.clone();
        aplicar(
            &[Operacion::Exposicion { pasos: 0.0 }, Operacion::Contraste { cantidad: 0.0 }],
            &mut img,
        );
        assert_eq!(img.datos(), partida.datos());
    }

    #[test]
    fn el_orden_canonico_pone_la_exposicion_antes_que_el_contraste() {
        assert!(
            Operacion::Exposicion { pasos: 1.0 }.orden()
                < Operacion::Contraste { cantidad: 1.0 }.orden()
        );
    }
}
