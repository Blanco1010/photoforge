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
    fn el_gris_medio_fotografico_cae_donde_debe() {
        // 0,18 lineal son 118 de 255. Es el número que usa todo el mundo para
        // comprobar que una cadena de color no se ha torcido.
        assert_eq!(tabla_salida().a_byte(0.18), 118);
        // En 16 bits, el mismo gris medio: 0,4614 de sRGB por 65535.
        let u = TablaSalida::a_u16(0.18);
        assert!((30_200..=30_270).contains(&u), "gris medio en 16 bits salió {u}");
    }
}
