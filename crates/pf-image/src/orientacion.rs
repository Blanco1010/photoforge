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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Orientacion(u8);

/// Lo normal es que una foto no haya que girarla.
///
/// Se escribe a mano porque el `Default` que genera el compilador daría
/// `Orientacion(0)`, que no es ninguna de las ocho y se trataría como
/// «derecha» por el camino de los números imposibles: funcionaría, pero por
/// accidente.
impl Default for Orientacion {
    fn default() -> Self {
        Orientacion::DERECHA
    }
}

impl Orientacion {
    /// La foto tal como está: no hay nada que hacer.
    pub const DERECHA: Orientacion = Orientacion(1);
    /// Los cinco gestos que el usuario puede pedir.
    ///
    /// Se escriben enteros y no abreviados porque es el vocabulario de la
    /// casa: `describir` ya dice «espejo horizontal» con todas sus letras.
    pub const GIRO_DERECHA: Orientacion = Orientacion(6);
    pub const GIRO_IZQUIERDA: Orientacion = Orientacion(8);
    pub const MEDIA_VUELTA: Orientacion = Orientacion(3);
    pub const ESPEJO_HORIZONTAL: Orientacion = Orientacion(2);
    pub const ESPEJO_VERTICAL: Orientacion = Orientacion(4);

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
    pub fn pasos_publicos(&self) -> (bool, bool, bool) {
        self.pasos()
    }

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

    /// ¿Esta orientación intercambia el ancho por el alto?
    pub fn traspone(&self) -> bool {
        self.pasos().0
    }

    /// La orientación que deshace esta.
    ///
    /// Seis de las ocho son su propia inversa —los espejos, el 180°, la
    /// traspuesta, la transversa y la identidad—; solo los dos giros de 90°
    /// se deshacen el uno al otro.
    pub fn inversa(self) -> Orientacion {
        match self.0 {
            6 => Orientacion::GIRO_IZQUIERDA,
            8 => Orientacion::GIRO_DERECHA,
            _ => self,
        }
    }

    /// La orientación que resulta de aplicar `self` y DESPUÉS `otra`.
    ///
    /// # De dónde sale esta fórmula
    ///
    /// No es una tabla de 8×8 copiada de ningún sitio: son 64 números que
    /// nadie puede verificar leyéndolos, y el que esté mal se descubre con
    /// una foto girada raro seis meses después. Sale de la descomposición que
    /// ya usa [`Orientacion::pasos`] —espejos primero, trasposición después—
    /// trabajando en coordenadas normalizadas:
    ///
    /// - Si la primera NO traspone, los dos espejos de cada eje se suman por
    ///   separado y la trasposición es la de la segunda.
    /// - Si la primera SÍ traspone, la segunda ve los ejes cambiados, así que
    ///   su espejo horizontal actúa sobre lo que era vertical y al revés; y
    ///   trasponer dos veces se anula, de ahí el `!t2`.
    ///
    /// La red que la sostiene no es este comentario: es la prueba que
    /// comprueba los 64 pares contra `enderezar`, que ya funciona.
    pub fn componer(self, otra: Orientacion) -> Orientacion {
        let (t1, h1, v1) = self.pasos();
        let (t2, h2, v2) = otra.pasos();
        if t1 {
            desde_pasos(!t2, h1 ^ v2, v1 ^ h2)
        } else {
            desde_pasos(t2, h1 ^ h2, v1 ^ v2)
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

/// La orientación que corresponde a estos tres pasos.
///
/// Es la inversa de [`Orientacion::pasos`], y es biyectiva porque los ocho
/// tríos que devuelve aquel método son los ocho distintos.
fn desde_pasos(trasponer: bool, espejo_h: bool, espejo_v: bool) -> Orientacion {
    Orientacion::desde_exif(match (trasponer, espejo_h, espejo_v) {
        (false, false, false) => 1,
        (false, true, false) => 2,
        (false, true, true) => 3,
        (false, false, true) => 4,
        (true, false, false) => 5,
        (true, false, true) => 6,
        (true, true, true) => 7,
        (true, true, false) => 8,
    })
}

/// Los seis coeficientes del mapa afín de coordenadas CONTINUAS de la foto
/// enderezada a coordenadas continuas del original.
///
/// `[ax, bx, cx, ay, by, cy]`, con `ox = ax·u + bx·v + cx` y
/// `oy = ay·u + by·v + cy`.
///
/// Son SEIS y no cuatro porque las cuatro orientaciones que trasponen hacen
/// que `ox` dependa de `v` y `oy` de `u`. Con cuatro coeficientes, un giro de
/// 90° pondría el degradado en el eje equivocado, y eso no se ve hasta tener
/// una foto girada con una franja delante.
pub fn afin_a_origen(pasos: (bool, bool, bool), ancho: i64, alto: i64) -> [f32; 6] {
    let (trasponer, espejo_h, espejo_v) = pasos;
    let (a, b) = (ancho as f32, alto as f32);
    // Deshacer la trasposición: lo que en la vista es `u` pasa a ser `v`.
    let (mut ax, mut bx, mut ay, mut by) = if trasponer {
        (0.0, 1.0, 1.0, 0.0)
    } else {
        (1.0, 0.0, 0.0, 1.0)
    };
    let (mut cx, mut cy) = (0.0f32, 0.0f32);
    if espejo_h {
        ax = -ax;
        bx = -bx;
        cx = a - cx;
    }
    if espejo_v {
        ay = -ay;
        by = -by;
        cy = b - cy;
    }
    [ax, bx, cx, ay, by, cy]
}

/// Dónde cae, en el original, el PUNTO continuo (x, y) de la foto enderezada.
///
/// # Ojo a la diferencia con `indice_en_origen`, que es la trampa del archivo
///
/// Con ÍNDICES, el espejo es `ancho − 1 − x`: el píxel 0 reflejado es el
/// W−1. Con coordenadas CONTINUAS es `ancho − x`, porque una coordenada
/// nombra una posición en la rejilla y el centro del píxel `i` está en
/// `i + 0,5`: el centro del 0 es 0,5 y reflejado tiene que dar W−0,5. Con la
/// fórmula de los índices daría W−1,5, que es el centro del píxel W−2.
///
/// Dicho sin inflarlo: medio píxel en una rampa repartida sobre novecientos
/// no se ve. La fórmula correcta se escribe porque cuesta cero y porque
/// tiene que vivir pegada a la otra para que nadie «arregle» una
/// convirtiéndola en la otra.
pub fn punto_en_origen(
    pasos: (bool, bool, bool),
    ancho: i64,
    alto: i64,
    x: f32,
    y: f32,
) -> (f32, f32) {
    let [ax, bx, cx, ay, by, cy] = afin_a_origen(pasos, ancho, alto);
    (ax * x + bx * y + cx, ay * x + by * y + cy)
}

/// El camino de vuelta: dónde cae, en la foto enderezada, un punto del
/// original.
///
/// Sale de aplicar la orientación INVERSA sobre la rejilla de la vista, así
/// que el álgebra existe una sola vez y no hay una segunda copia que se
/// pueda desincronizar.
pub fn punto_en_vista(o: Orientacion, original: Dimensiones, x: f32, y: f32) -> (f32, f32) {
    let vista = o.dimensiones_resultantes(original);
    punto_en_origen(
        o.inversa().pasos_publicos(),
        vista.ancho as i64,
        vista.alto as i64,
        x,
        y,
    )
}

/// De dónde sale, en el original, el píxel (x, y) de la foto ya enderezada.
///
/// # Por qué esto trabaja con ÍNDICES y no con un rectángulo
///
/// El primer intento fue traducir la región que pide el lienzo a una región
/// equivalente en el original, y girar después el trozo muestreado. Es lo
/// evidente y **no puede ser exacto**: reflejar manda el intervalo [i, i+1)
/// al (W−i−1, W−i], o sea cerrado por el otro lado, así que cualquier regla
/// de redondeo —`floor` o `ceil`— se va un píxel justo cuando el centro de
/// muestreo cae en un entero exacto. Lo cazó la prueba de equivalencia con
/// una región de las elegidas a propósito para eso.
///
/// Un giro es una PERMUTACIÓN de píxeles, así que la traducción exacta existe
/// y es esta: se muestrea sobre la rejilla de la foto girada, como si
/// estuviera ahí, y para cada índice se mira de dónde sale. Exacto por
/// construcción, y de regalo se ahorra girar 17 MB en cada cuadro.
///
/// `ancho` y `alto` son los del ORIGINAL. Los pasos son los mismos y en el
/// mismo orden que el bucle de [`enderezar`], que es de donde salen.
#[inline]
pub fn indice_en_origen(
    pasos: (bool, bool, bool),
    ancho: i64,
    alto: i64,
    x: i64,
    y: i64,
) -> (i64, i64) {
    let (trasponer, espejo_h, espejo_v) = pasos;
    let (mut ox, mut oy) = if trasponer { (y, x) } else { (x, y) };
    if espejo_h {
        ox = ancho - 1 - ox;
    }
    if espejo_v {
        oy = alto - 1 - oy;
    }
    (ox, oy)
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

    /// Una imagen con un valor distinto en cada píxel, para poder seguirlos.
    fn degradado(ancho: u32, alto: u32) -> Imagen {
        let dim = Dimensiones::nueva(ancho, alto).unwrap();
        let datos: Vec<f32> = (0..dim.pixeles())
            .flat_map(|i| [i as f32; CANALES])
            .collect();
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


    /// Las ocho orientaciones, para recorrerlas en las pruebas exhaustivas.
    const TODAS: [u8; 8] = [1, 2, 3, 4, 5, 6, 7, 8];

    #[test]
    fn componer_coincide_con_aplicar_las_dos_de_seguidas() {
        // LA prueba de la composición. Son los 64 pares contra `enderezar`,
        // que ya estaba probado: si la fórmula tiene un espejo cambiado de
        // eje, aquí salta. Los cuatro casos con espejo son los que se
        // escriben al revés, y sin esta prueba el error se vería como «la
        // foto gira al contrario» solo en ellos.
        for a in TODAS {
            for b in TODAS {
                let (oa, ob) = (Orientacion::desde_exif(a as u16), Orientacion::desde_exif(b as u16));
                let dos_pasos = enderezar(enderezar(numerada(), oa).unwrap(), ob).unwrap();
                let compuesta = enderezar(numerada(), oa.componer(ob)).unwrap();
                assert_eq!(
                    valores(&dos_pasos),
                    valores(&compuesta),
                    "componer({a}, {b}) = {} no coincide",
                    oa.componer(ob).numero()
                );
                assert_eq!(dos_pasos.dim(), compuesta.dim(), "las dimensiones no cuadran en ({a}, {b})");
            }
        }
    }

    #[test]
    fn cuatro_giros_a_la_derecha_vuelven_al_principio() {
        let mut o = Orientacion::DERECHA;
        for _ in 0..4 {
            o = o.componer(Orientacion::GIRO_DERECHA);
        }
        assert!(o.no_hay_que_hacer_nada());
        // Y un espejo encima de un giro da la traspuesta, que es la identidad
        // concreta que caza una composición escrita del revés.
        assert_eq!(
            Orientacion::ESPEJO_HORIZONTAL.componer(Orientacion::GIRO_DERECHA).numero(),
            // La transversa, no la traspuesta: espejar y luego girar a la
            // derecha NO es lo mismo que girar y luego espejar. Es justo el
            // par que se escribe al revés, y por eso está aquí con su número.
            7
        );
    }

    #[test]
    fn desde_pasos_es_la_inversa_de_pasos() {
        for v in TODAS {
            let o = Orientacion::desde_exif(v as u16);
            let (t, h, ve) = o.pasos();
            assert_eq!(desde_pasos(t, h, ve).numero(), v);
        }
    }

    #[test]
    fn muestrear_girado_da_lo_mismo_que_girar_la_foto_y_muestrear() {
        // LA prueba de `indice_en_origen`, y la que de verdad importa: pedir
        // un recorte de la foto girada tiene que dar EXACTAMENTE lo mismo que
        // girar la foto entera y recortar de ella.
        //
        // Las regiones están elegidas a propósito, no al azar: la foto
        // entera, una esquina, una con decimales, y dos cuyos centros caen en
        // enteros exactos, que es donde el intento anterior —traducir el
        // rectángulo en vez del índice— se iba un píxel.
        let img = degradado(8, 6);
        let casos: [(pf_core::Region, u32, u32); 5] = [
            (pf_core::Region { x: 0.0, y: 0.0, ancho: 8.0, alto: 6.0 }, 8, 6),
            (pf_core::Region { x: 0.0, y: 0.0, ancho: 4.0, alto: 3.0 }, 4, 3),
            (pf_core::Region { x: 1.5, y: 2.25, ancho: 3.5, alto: 2.5 }, 7, 5),
            (pf_core::Region { x: 0.5, y: 0.5, ancho: 4.0, alto: 4.0 }, 4, 4),
            (pf_core::Region { x: 2.0, y: 1.0, ancho: 3.0, alto: 2.0 }, 6, 4),
        ];

        for v in TODAS {
            let o = Orientacion::desde_exif(v as u16);
            let girada = enderezar(img.clone(), o).unwrap();
            for (region, ancho, alto) in casos {
                let destino = Dimensiones::nueva(ancho, alto).unwrap();
                let largo = crate::muestreo::muestrear(&girada, region, destino);
                let corto = crate::muestreo::muestrear_girado(&img, region, destino, o);
                assert_eq!(largo.dim(), corto.dim(), "orientación {v}, región {region:?}");
                assert_eq!(
                    valores(&largo),
                    valores(&corto),
                    "orientación {v}, región {region:?}: los dos caminos no coinciden"
                );
            }
        }
    }


    #[test]
    fn la_inversa_deshace_la_orientacion() {
        for v in TODAS {
            let o = Orientacion::desde_exif(v as u16);
            assert!(o.componer(o.inversa()).no_hay_que_hacer_nada(), "la inversa de {v} falla");
            assert!(o.inversa().componer(o).no_hay_que_hacer_nada());
        }
    }

    #[test]
    fn el_punto_continuo_cae_dentro_del_pixel_que_dice_el_indice() {
        // Las dos funciones tienen que estar de acuerdo, y es justo donde se
        // mete la pata: el espejo de un ÍNDICE es `ancho−1−x` y el de una
        // COORDENADA es `ancho−x`.
        let (ancho, alto) = (7i64, 5i64);
        for v in TODAS {
            let o = Orientacion::desde_exif(v as u16);
            let pasos = o.pasos_publicos();
            let vista = o.dimensiones_resultantes(Dimensiones::nueva(7, 5).unwrap());
            for y in 0..vista.alto as i64 {
                for x in 0..vista.ancho as i64 {
                    let (ix, iy) = indice_en_origen(pasos, ancho, alto, x, y);
                    let (px, py) =
                        punto_en_origen(pasos, ancho, alto, x as f32 + 0.5, y as f32 + 0.5);
                    assert_eq!(
                        (px.floor() as i64, py.floor() as i64),
                        (ix, iy),
                        "orientación {v}, píxel ({x}, {y}): el punto {px},{py} no cae en {ix},{iy}"
                    );
                }
            }
        }
    }

    #[test]
    fn el_punto_va_y_vuelve_entre_la_vista_y_el_original() {
        let original = Dimensiones::nueva(7, 5).unwrap();
        for v in TODAS {
            let o = Orientacion::desde_exif(v as u16);
            let vista = o.dimensiones_resultantes(original);
            for i in 0..=10 {
                for j in 0..=10 {
                    let x = vista.ancho as f32 * i as f32 / 10.0;
                    let y = vista.alto as f32 * j as f32 / 10.0;
                    let (ox, oy) = punto_en_origen(o.pasos_publicos(), 7, 5, x, y);
                    let (vx, vy) = punto_en_vista(o, original, ox, oy);
                    assert!(
                        (vx - x).abs() < 1e-4 && (vy - y).abs() < 1e-4,
                        "orientación {v}: ({x}, {y}) -> ({ox}, {oy}) -> ({vx}, {vy})"
                    );
                }
            }
        }
    }
    #[test]
    fn un_numero_de_orientacion_imposible_se_trata_como_derecha() {
        for v in [0u16, 9, 255, 65535] {
            assert!(Orientacion::desde_exif(v).no_hay_que_hacer_nada(), "el {v} no se ignoró");
        }
    }
}
