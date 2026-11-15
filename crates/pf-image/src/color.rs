//! Las dos conversiones que separan los números de un archivo de la luz real.
//!
//! Un JPEG no guarda luz: guarda luz pasada por la curva de sRGB, que dedica
//! más códigos a las sombras porque es ahí donde el ojo distingue. Para editar
//! hay que deshacer esa curva, y para enseñar el resultado hay que volver a
//! ponerla. Todo lo que hay en este archivo es eso, hecho rápido.

use std::sync::LazyLock;

/// De sRGB (0..1) a luz lineal (0..1). La fórmula de la norma IEC 61966-2-1.
///
/// El tramo recto de abajo no es un adorno: sin él la curva tendría pendiente
/// infinita en el cero y los valores más oscuros se volverían inestables.
pub fn srgb_a_lineal(v: f32) -> f32 {
    if v <= 0.040_448_237 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

/// De luz lineal (0..1) a sRGB (0..1). La inversa exacta de la anterior.
pub fn lineal_a_srgb(v: f32) -> f32 {
    if v <= 0.003_130_8 {
        v * 12.92
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    }
}

/// La rampa suave que usan todas las máscaras de revelado.
///
/// Es el `smoothstep` de toda la vida: vale 0 hasta `t = 0`, 1 a partir de
/// `t = 1`, y entre medias sube con pendiente cero en los dos extremos.
///
/// Esa pendiente cero es justo el motivo de usarla y no una rampa recta: una
/// recta deja una esquina donde empieza y donde acaba, y en una foto una
/// esquina en la curva de tono se ve como una raya en un degradado —el cielo
/// de la tarde es el sitio donde siempre sale—.
#[inline]
pub fn suave(t: f32) -> f32 {
    let u = t.clamp(0.0, 1.0);
    u * u * (3.0 - 2.0 * u)
}

/// Cuánta luz tiene un píxel, con los coeficientes de Rec. 709.
///
/// Se aplica sobre valores LINEALES, que es donde esos tres números tienen
/// sentido físico: dicen cuánto contribuye cada primario a la luz total que
/// sale de la pantalla. El verde pesa siete veces más que el azul porque el
/// ojo es así, no porque sea una convención.
///
/// Vive aquí y no en cada sitio que la necesita porque ya son tres: la
/// exposición automática de un RAW, el histograma y el revelado.
#[inline]
pub fn luminancia(r: f32, v: f32, a: f32) -> f32 {
    0.2126 * r + 0.7152 * v + 0.0722 * a
}

/// Donde empieza a doblarse la curva de luces.
///
/// Por debajo de este valor no se toca nada. Se eligió 0,75 porque es donde
/// una foto deja de tener detalle que juzgar y empieza a tener brillo: el
/// gris medio está en 0,18 y una piel clara bien expuesta ronda 0,45, así que
/// a 0,75 ya solo quedan reflejos, cielos y fuentes de luz.
pub const CODO_DE_LUCES: f32 = 0.75;

/// Dobla las luces en vez de cortarlas.
///
/// # Qué problema resuelve
///
/// Al subir la exposición de un RAW, las luces se pasan de 1,0 y se recortan:
/// un cielo con nubes se convierte en una mancha blanca plana, y el borde
/// entre lo que se recortó y lo que no se ve como un escalón. Es el aspecto
/// que delata a una foto mal revelada.
///
/// Esta curva hace lo que hacía la película: en vez de cortar en 1,0, se va
/// acercando sin llegar nunca, así que por mucho que se suba la exposición
/// siempre queda algo de diferencia entre dos luces distintas.
///
/// # Por qué con un codo y no una curva en todo el rango
///
/// Las curvas tipo Reinhard —`x / (1 + x)` y sus parientes— comprimen también
/// los medios tonos: aplicarlas oscurece una foto que ya estaba bien. Con un
/// codo, todo lo que está por debajo de [`CODO_DE_LUCES`] **sale exactamente
/// igual que entró**, y solo se dobla lo que iba a quemarse. Activar la curva
/// no puede estropear la parte de la foto que ya te gustaba.
///
/// La forma es exponencial porque encaja con pendiente 1 justo en el codo: la
/// curva y la recta se tocan sin esquina, así que no aparece una banda en la
/// transición.
///
/// # Hasta dónde llega de verdad
///
/// Sobre el papel la curva se acerca a 1,0 sin tocarlo nunca. En `f32` no, y
/// conviene saber dónde está el límite de verdad:
///
/// - Hasta una entrada de **3,1** —un paso y medio por encima del blanco—
///   dos valores separados por una milésima siguen saliendo distintos.
/// - Entre 3,1 y **4,7** la curva sigue creciendo, pero ya solo distingue
///   diferencias cada vez más gruesas: la mantisa de un `f32` se queda sin
///   bits cerca de 1,0.
/// - A partir de 4,7 el resultado es exactamente 1,0.
///
/// Es decir: la curva salva lo que se puede salvar de un cielo sobreexpuesto
/// un par de pasos, y por encima de eso no hay nada que salvar porque la
/// información ya no cabe en el número. Se dice aquí porque es el tipo de
/// límite que, si no está escrito, alguien descubre un día con una foto rara
/// y cree que es un fallo.
#[inline]
pub fn doblar_luces(v: f32) -> f32 {
    if v <= CODO_DE_LUCES {
        return v;
    }
    let margen = 1.0 - CODO_DE_LUCES;
    CODO_DE_LUCES + margen * (1.0 - (-(v - CODO_DE_LUCES) / margen).exp())
}

/// Tabla de entrada para los archivos de 8 bits.
///
/// Por qué una tabla: `srgb_a_lineal` lleva una `powf`, y una `powf` cuesta
/// del orden de veinte veces más que leer de memoria. Un JPEG de 50 Mpx son
/// 150 millones de llamadas. Como solo hay 256 valores posibles de entrada, la
/// tabla da el resultado EXACTO, no una aproximación: aquí no se cambia
/// precisión por velocidad, se cambia trabajo repetido por trabajo hecho una
/// vez.
static ENTRADA_8: LazyLock<[f32; 256]> = LazyLock::new(|| {
    let mut t = [0.0f32; 256];
    for (i, v) in t.iter_mut().enumerate() {
        *v = srgb_a_lineal(i as f32 / 255.0);
    }
    t
});

pub fn tabla_entrada_8() -> &'static [f32; 256] {
    &ENTRADA_8
}

/// Tabla de entrada para los archivos de 16 bits.
///
/// 65.536 entradas son 256 KB, que caben de sobra y vuelven a dar el valor
/// exacto. Un TIFF de 16 bits y 50 Mpx son 150 millones de `powf` que no se
/// hacen. La tabla se construye la primera vez que se abre un archivo de 16
/// bits y no antes, que es lo que hace `LazyLock`: quien solo abra JPEG no
/// paga esos 256 KB.
static ENTRADA_16: LazyLock<Vec<f32>> =
    LazyLock::new(|| (0..=u16::MAX).map(|i| srgb_a_lineal(i as f32 / 65535.0)).collect());

pub fn tabla_entrada_16() -> &'static [f32] {
    &ENTRADA_16
}

/// Número de escalones de la tabla de salida.
///
/// La salida es al revés que la entrada: entran valores de coma flotante, que
/// son infinitos, y sale un byte. Aquí la tabla SÍ es una aproximación, así
/// que el tamaño hay que justificarlo.
///
/// El peor caso está en el negro, donde la curva de sRGB tiene pendiente
/// 12,92: un error de 1/4096 en lineal se convierte en 12,92/4096 = 0,0032 en
/// sRGB, que son 0,8 de los 255 niveles de salida. Con 4096 escalones el error
/// se queda por debajo de un nivel de 8 bits en todo el rango, y por debajo de
/// un nivel no hay banda que ver porque el byte ya no puede distinguirlo.
const ESCALONES_SALIDA: usize = 4096;

pub struct TablaSalida {
    valores: [u8; ESCALONES_SALIDA + 1],
}

impl TablaSalida {
    /// Convierte un valor lineal en un byte de sRGB.
    ///
    /// Recorta por abajo y por arriba, y eso es deliberado: la exposición
    /// puede dejar valores negativos o por encima de 1,0, y son válidos
    /// mientras viven en el documento. Lo que no se puede es enseñarlos en una
    /// pantalla, así que el recorte ocurre aquí, en el último paso, y nunca
    /// sobre los datos guardados.
    // El `!(x > 0.0)` de abajo es a propósito y clippy se queja de él: con un
    // `x <= 0.0` el NaN se escaparía, porque NO es menor ni igual que nada.
    // Negar el «mayor que» es lo único que coge el NaN y el negativo de una
    // sola comparación, sin un `is_nan()` aparte en el bucle más caliente del
    // programa.
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    #[inline]
    pub fn a_byte(&self, lineal: f32) -> u8 {
        // El NaN se trata como negro. Un NaN suelto en un buffer de píxeles
        // pinta basura aleatoria y es imposible de rastrear; mejor un punto
        // negro, que se ve y se puede contar.
        if !(lineal > 0.0) {
            return 0;
        }
        if lineal >= 1.0 {
            return 255;
        }
        let i = (lineal * ESCALONES_SALIDA as f32) as usize;
        self.valores[i]
    }

    /// La versión de 16 bits para exportar a TIFF, calculada sin tabla.
    ///
    /// Aquí no se usa la tabla porque un escalón de la tabla vale casi tres
    /// niveles de 16 bits: la aproximación que es invisible en un byte se
    /// convierte en bandas visibles en un TIFF que luego alguien va a seguir
    /// editando. Exportar pasa una vez por archivo, así que la `powf` se paga.
    #[allow(clippy::neg_cmp_op_on_partial_ord)] // mismo motivo que en `a_byte`
    #[inline]
    pub fn a_u16(lineal: f32) -> u16 {
        if !(lineal > 0.0) {
            return 0;
        }
        if lineal >= 1.0 {
            return u16::MAX;
        }
        (lineal_a_srgb(lineal) * 65535.0 + 0.5) as u16
    }
}

static SALIDA: LazyLock<TablaSalida> = LazyLock::new(|| {
    let mut valores = [0u8; ESCALONES_SALIDA + 1];
    for (i, v) in valores.iter_mut().enumerate() {
        let lineal = i as f32 / ESCALONES_SALIDA as f32;
        *v = (lineal_a_srgb(lineal) * 255.0 + 0.5).clamp(0.0, 255.0) as u8;
    }
    TablaSalida { valores }
});

pub fn tabla_salida() -> &'static TablaSalida {
    &SALIDA
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn las_dos_curvas_son_inversas_la_una_de_la_otra() {
        for i in 0..=1000 {
            let v = i as f32 / 1000.0;
            let ida_y_vuelta = lineal_a_srgb(srgb_a_lineal(v));
            assert!((ida_y_vuelta - v).abs() < 1e-5, "en {v} salió {ida_y_vuelta}");
        }
    }

    #[test]
    fn la_tabla_de_8_bits_da_el_mismo_valor_que_la_formula() {
        // La tabla no es una aproximación: tiene que coincidir bit a bit.
        for i in 0..256usize {
            assert_eq!(tabla_entrada_8()[i], srgb_a_lineal(i as f32 / 255.0));
        }
    }

    #[test]
    fn la_tabla_de_salida_no_se_desvia_mas_de_un_nivel() {
        // Esta es la prueba que justifica los 4096 escalones. Si alguien los
        // baja a 256 para ahorrar memoria, esta prueba lo coge.
        let t = tabla_salida();
        for i in 0..=10_000 {
            let lineal = i as f32 / 10_000.0;
            let exacto = (lineal_a_srgb(lineal) * 255.0 + 0.5) as i32;
            let tabla = t.a_byte(lineal) as i32;
            assert!((exacto - tabla).abs() <= 1, "en {lineal}: exacto {exacto}, tabla {tabla}");
        }
    }

    #[test]
    fn los_valores_imposibles_no_pintan_basura() {
        let t = tabla_salida();
        assert_eq!(t.a_byte(f32::NAN), 0);
        assert_eq!(t.a_byte(-5.0), 0);
        assert_eq!(t.a_byte(f32::INFINITY), 255);
        assert_eq!(t.a_byte(100.0), 255);
    }

    #[test]
    fn la_rampa_suave_tiene_pendiente_cero_en_los_dos_extremos() {
        // Es la propiedad entera: sin ella aparece una raya en los degradados.
        assert_eq!(suave(-1.0), 0.0);
        assert_eq!(suave(0.0), 0.0);
        assert_eq!(suave(1.0), 1.0);
        assert_eq!(suave(2.0), 1.0);
        assert!((suave(0.5) - 0.5).abs() < 1e-6);
        let d = 1e-3;
        assert!(suave(d) / d < 0.01, "la rampa arranca con pendiente apreciable");
        assert!((1.0 - suave(1.0 - d)) / d < 0.01, "la rampa termina con pendiente apreciable");
    }

    #[test]
    fn la_luminancia_de_un_gris_es_ese_mismo_gris() {
        for v in [0.0f32, 0.18, 0.5, 1.0, 3.0] {
            assert!((luminancia(v, v, v) - v).abs() < 1e-6);
        }
        // Y el verde pesa más que los otros dos juntos.
        assert!(luminancia(0.0, 1.0, 0.0) > luminancia(1.0, 0.0, 1.0));
    }

    #[test]
    fn la_curva_de_luces_no_toca_lo_que_esta_por_debajo_del_codo() {
        // Es la propiedad que justifica el codo: activar la curva no puede
        // cambiar los medios tonos de una foto que ya estaba bien.
        for v in [0.0, 0.01, 0.18, 0.45, 0.74, CODO_DE_LUCES] {
            assert_eq!(doblar_luces(v), v, "la curva movió {v}");
        }
    }

    #[test]
    fn la_curva_de_luces_nunca_se_pasa_de_uno() {
        for v in [0.8, 1.0, 1.5, 4.0, 100.0, 1e6, f32::MAX] {
            let y = doblar_luces(v);
            assert!(y <= 1.0, "con {v} salió {y}, que se pasa del blanco");
            assert!(y > CODO_DE_LUCES);
        }
    }

    #[test]
    fn la_curva_de_luces_salva_el_rango_que_dice_salvar() {
        // Hasta 4,5 —dos pasos y pico por encima del blanco— dos luces
        // distintas tienen que seguir saliendo distintas. Esta prueba es la
        // que fija el límite que documenta `doblar_luces`.
        for v in [1.0f32, 1.5, 2.0, 3.0, 4.0] {
            assert!(
                doblar_luces(v) < doblar_luces(v + 0.4),
                "a la altura de {v} la curva ya no distingue"
            );
        }
        // Y a partir de ahí se reconoce que junta: no se promete lo que no se
        // cumple.
        assert_eq!(doblar_luces(8.0), doblar_luces(80.0));
    }

    #[test]
    fn la_curva_de_luces_es_creciente_y_sin_esquina() {
        // Creciente: dos luces distintas siguen siendo distintas, que es todo
        // el objetivo. Sin esquina: la pendiente a los dos lados del codo
        // tiene que ser casi la misma, o se vería una banda justo ahí.
        // Hasta 3,1 crece de verdad, milésima a milésima.
        let mut anterior = -1.0;
        for i in 0..3100 {
            let v = i as f32 / 1000.0;
            let y = doblar_luces(v);
            assert!(y > anterior, "la curva dejó de crecer en {v}");
            anterior = y;
        }
        // Y de ahí arriba, al menos nunca baja. Lo que no puede pasar es que
        // una luz más alta salga más oscura: eso sí se vería, como un halo
        // raro en el centro de una fuente de luz.
        for i in 3100..20_000 {
            let v = i as f32 / 1000.0;
            let y = doblar_luces(v);
            assert!(y >= anterior, "la curva bajó en {v}");
            anterior = y;
        }
        let d = 1e-4;
        let antes = (doblar_luces(CODO_DE_LUCES) - doblar_luces(CODO_DE_LUCES - d)) / d;
        let despues = (doblar_luces(CODO_DE_LUCES + d) - doblar_luces(CODO_DE_LUCES)) / d;
        assert!((antes - despues).abs() < 0.01, "esquina en el codo: {antes} vs {despues}");
    }

    #[test]
    fn el_gris_medio_fotografico_cae_donde_debe() {
        // 0,18 lineal son 118 de 255. Es el número que usa todo el mundo para
        // comprobar que una cadena de color no se ha torcido.
        assert_eq!(tabla_salida().a_byte(0.18), 118);
        // En 16 bits, el mismo gris medio: 0,4614 de sRGB por 65535.
        let u = TablaSalida::a_u16(0.18);
        assert!((30_200..=30_270).contains(&u), "gris medio en 16 bits salió {u}");
    }
}
