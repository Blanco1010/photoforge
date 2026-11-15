//! Las operaciones de revelado.

use pf_image::{Imagen, color};

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
    ///
    /// Es una multiplicación y nada más, y esa sencillez es el argumento
    /// entero a favor de trabajar en lineal: un paso de exposición es el
    /// doble de fotones, y en lineal el doble de fotones es el doble de
    /// número. En sRGB habría que deshacer la curva, multiplicar y volver a
    /// ponerla, tres veces por píxel y por canal.
    ///
    /// No se recorta por arriba a propósito: un valor de 1,8 después de subir
    /// la exposición sigue siendo un dato, y bajar el contraste después puede
    /// devolverlo al rango visible. El recorte ocurre solo al pintar o al
    /// exportar.
    Exposicion { pasos: f32 },
    /// Abre o cierra lo más oscuro, de -100 a +100.
    ///
    /// Ver [`sombras`] para la fórmula y [`mascara_sombras`] para la zona.
    Sombras { cantidad: f32 },
    /// Aclara o recupera lo más brillante, de -100 a +100.
    ///
    /// Ver [`luces`] y [`mascara_luces`].
    Luces { cantidad: f32 },
    /// Decide dónde cae el negro, de -100 a +100.
    ///
    /// Ver [`negros`] y [`mascara_negros`].
    Negros { cantidad: f32 },
    /// Decide dónde cae el blanco, de -100 a +100.
    ///
    /// Ver [`blancos`] y [`mascara_blancos`].
    Blancos { cantidad: f32 },
    /// Contraste, de -100 a +100. El 0 no toca nada.
    ///
    /// # Por qué el contraste NO se aplica en lineal
    ///
    /// Es la excepción a la regla de la casa, y tiene motivo. En luz lineal,
    /// el gris medio está en 0,18 y las sombras se amontonan todas por debajo
    /// de 0,05: estirar alrededor de 0,18 mueve muchísimo las luces y casi
    /// nada las sombras, así que el primer 20 % del deslizador no parecería
    /// hacer nada y el último 20 % quemaría el cielo. El deslizador tiene que
    /// sentirse parejo.
    ///
    /// Así que se aplica sobre el valor perceptual —el mismo que vería la
    /// pantalla—, con el pivote en 0,5, y el resultado se devuelve a lineal
    /// para que las operaciones siguientes sigan trabajando con luz. Las
    /// conversiones se extienden fuera de 0..1 sin recortar: un 1,3 que entra
    /// sale por el otro lado en vez de quedarse pegado al blanco.
    Contraste { cantidad: f32 },
    /// Dobla las luces en vez de cortarlas. Es una casilla, no un
    /// deslizador.
    ///
    /// Se aplica canal a canal y no sobre la luminancia a propósito:
    /// comprimir los tres canales por igual mantiene el tono, mientras que
    /// comprimir solo la luminancia dejaría los canales que ya iban pasados
    /// por encima de los otros y teñiría las luces.
    ///
    /// Por qué sin deslizador: lo único que se podría regular es dónde
    /// empieza el codo, y eso es una decisión que nadie puede juzgar mirando
    /// la foto —el efecto solo se ve en el puñado de píxeles más brillantes—.
    /// Un control que no se puede juzgar es un control que se deja donde
    /// venía, así que vale más una casilla honrada.
    ///
    /// Por qué lleva `activa` en vez de estar o no estar en la lista: así
    /// apagarla pasa por la MISMA puerta que devolver un deslizador a cero
    /// —`Documento::ajustar` con algo neutro—, en vez de necesitar un camino
    /// aparte para quitar operaciones que solo usaría esta.
    #[serde(rename = "curva")]
    CurvaDeTono { activa: bool },
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
            Operacion::Sombras { .. } => "sombras",
            Operacion::Luces { .. } => "luces",
            Operacion::Negros { .. } => "negros",
            Operacion::Blancos { .. } => "blancos",
            Operacion::Contraste { .. } => "contraste",
            Operacion::CurvaDeTono { .. } => "curva",
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
            Operacion::Sombras { cantidad }
            | Operacion::Luces { cantidad }
            | Operacion::Negros { cantidad }
            | Operacion::Blancos { cantidad } => *cantidad == 0.0,
            Operacion::Contraste { cantidad } => *cantidad == 0.0,
            Operacion::CurvaDeTono { activa } => !*activa,
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
            Operacion::CurvaDeTono { .. } => 20,
            // El bloque tonal, con huecos de dos en dos para poder meter algo
            // en medio sin renumerar lo demás.
            //
            // Recuperar (luces y sombras) antes que decidir dónde caen los
            // extremos (blancos y negros): al revés, lo que acabas de
            // recortar ya no se puede levantar.
            //
            // Dentro de cada par el orden da IGUAL, y eso no es casualidad:
            // la máscara de luces se anula por debajo del medio tono y la de
            // sombras por encima, así que las dos no se tocan nunca. Lo mismo
            // con blancos y negros. Hay una prueba que lo fija. Como da
            // igual, se eligió el orden en que los enseña el panel, que es el
            // de Lightroom: de lo claro a lo oscuro.
            Operacion::Luces { .. } => 30,
            Operacion::Sombras { .. } => 32,
            Operacion::Blancos { .. } => 34,
            Operacion::Negros { .. } => 36,
            Operacion::Contraste { .. } => 40,
        }
    }
}


// ===========================================================================
// Los cuatro controles tonales
// ===========================================================================
//
// # Por qué actúan sobre la LUMINANCIA y no canal a canal
//
// «Aclarar las sombras» es una frase sobre el brillo, no sobre el rojo. Si
// cada canal se tratara por su cuenta, un rojo oscuro saturado se aclararía
// solo por el canal rojo y el color se iría hacia el pastel: la foto perdería
// saturación justo donde más se nota. Calculando una ganancia a partir de la
// luminancia y multiplicando los tres canales por ELLA, el tono se queda
// exactamente donde estaba.
//
// # Por qué la zona se decide en espacio perceptual
//
// La máscara tiene que preguntar «¿esto es una sombra?», y eso es una
// pregunta sobre cómo se ve, no sobre cuántos fotones hay. En luz lineal, lo
// que el ojo llama «medio tono» está en 0,18 y casi toda la foto se amontona
// por debajo de 0,2: una máscara repartida ahí no separaría nada. Es el mismo
// razonamiento por el que el contraste no se aplica en lineal.
//
// # Por qué no hay máscara espacial
//
// Lightroom y compañía miran también a los vecinos, y por eso pueden abrir la
// sombra del árbol sin tocar la chaqueta oscura del primer plano. Eso vale un
// desenfoque guiado por pasada, que es exactamente la clase de operación que
// este revelado no puede permitirse hoy —hay que leer los vecinos, y el
// muestreo de la vista ya ha tirado la mitad de ellos—. A cambio, estas
// máscaras NO pueden dar halos, que es el defecto clásico del otro camino.
// El tooltip lo dice: actúa por brillo y no por zonas.

/// Lo que mueven luces y sombras a tope, como fracción del camino al destino.
const FUERZA_ZONAL: f32 = 0.5;
/// Lo que mueven blancos y negros a tope, en valor perceptual.
const FUERZA_EXTREMO: f32 = 0.35;
/// El pie y el hombro que anclan las máscaras zonales en 0 y en 1.
const PIE: f32 = 0.08;
const HOMBRO: f32 = 0.08;
/// El medio tono perceptual: el pivote de todo el bloque.
const MEDIO: f32 = 0.5;
/// Cuánto se extienden las máscaras de los extremos hacia el centro.
const ANCHO: f32 = 0.75;

/// La zona de las sombras.
///
/// Sube desde 0 en el negro absoluto —ese es el `PIE`, y es lo que hace que
/// subir las sombras NO levante el punto negro— y baja hasta 0 en el medio
/// tono.
pub fn mascara_sombras(p: f32) -> f32 {
    color::suave(p / PIE) * (1.0 - color::suave(p / MEDIO))
}

/// La zona de las luces, simétrica: sube desde el medio tono y vuelve a 0 en
/// el blanco, que es lo que impide que subir las luces queme lo que no estaba
/// quemado.
pub fn mascara_luces(p: f32) -> f32 {
    color::suave((p - MEDIO) / MEDIO) * (1.0 - color::suave((p - (1.0 - HOMBRO)) / HOMBRO))
}

/// La zona de los negros: máxima en el negro y nula a partir del medio tono.
/// Aquí NO hay pie, y es el punto entero del control: este sí mueve el negro.
pub fn mascara_negros(p: f32) -> f32 {
    color::suave((MEDIO - p) / ANCHO)
}

/// La zona de los blancos. Sin hombro, por el mismo motivo.
pub fn mascara_blancos(p: f32) -> f32 {
    color::suave((p - MEDIO) / ANCHO)
}

/// Sombras: empuja la zona oscura hacia el medio tono, o hacia el negro.
///
/// Es una interpolación hacia un destino y no una suma, y de ahí sale la
/// garantía: por mucho que se suba, un tono no puede pasar del medio tono, y
/// por mucho que se baje no puede pasar del negro. Nada se sale del rango.
pub fn sombras(p: f32, t: f32) -> f32 {
    let destino = if t > 0.0 { MEDIO } else { 0.0 };
    p + FUERZA_ZONAL * t.abs() * mascara_sombras(p) * (destino - p)
}

/// Luces: lo mismo hacia el blanco o hacia el medio tono.
pub fn luces(p: f32, t: f32) -> f32 {
    let destino = if t > 0.0 { 1.0 } else { MEDIO };
    p + FUERZA_ZONAL * t.abs() * mascara_luces(p) * (destino - p)
}

/// Negros: una suma, no una interpolación.
///
/// Y esa es toda la diferencia con sombras: este SÍ puede sacar un tono por
/// debajo del negro, porque para eso está. Decidir dónde cae el negro
/// significa poder recortar.
pub fn negros(p: f32, t: f32) -> f32 {
    p + FUERZA_EXTREMO * t * mascara_negros(p)
}

/// Blancos: igual por el otro lado, y también puede recortar.
pub fn blancos(p: f32, t: f32) -> f32 {
    p + FUERZA_EXTREMO * t * mascara_blancos(p)
}

/// Aplica la lista de operaciones sobre una imagen, en el sitio.
///
/// Recibe la imagen ya muestreada al tamaño de la vista, no la original: ver
/// `pf_image::muestreo` para el porqué.
///
/// # Por qué esto no es un bucle por operación
///
/// Lo era, y no aguantaba. Cada operación recorría la imagen entera por su
/// cuenta, y lo que cuesta de verdad no son las multiplicaciones sino cuántas
/// veces se va y se vuelve de la memoria: una vista de 1.600 × 900 en RGB de
/// `f32` son 17 MB por pasada. Con tres operaciones se iban unos 12 ms solo en
/// eso; con los nueve controles que vienen serían cinco pasadas y el cuadro se
/// saldría de los 33 ms, o sea por debajo de los 30 cuadros por segundo de los
/// que presume el proyecto.
///
/// Ahora la lista se resuelve PRIMERO —en [`crate::plan::Plan`]— y se ejecuta
/// en una sola pasada, estén movidos tres deslizadores o nueve.
///
/// Lo que cada operación hace y por qué está en el comentario de su variante,
/// aquí arriba; lo que hace falta para componerlas, en `plan.rs`.
pub fn aplicar(operaciones: &[Operacion], img: &mut Imagen) {
    crate::plan::Plan::desde(operaciones).ejecutar(img);
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use pf_image::color;
    use pf_core::Dimensiones;
    use pf_image::CANALES;

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
    fn la_curva_de_tono_salva_las_luces_sin_tocar_los_medios() {
        let mut img = imagen(&[0.18, 0.5, 1.4, 4.0]);
        aplicar(&[Operacion::CurvaDeTono { activa: true }], &mut img);
        let v = canal_rojo(&img);
        assert_eq!(v[0], 0.18, "el gris medio no se puede mover");
        assert_eq!(v[1], 0.5, "medio punto tampoco");
        assert!(v[2] < 1.0 && v[2] > 0.75, "1,4 tenía que doblarse, salió {}", v[2]);
        assert!(v[3] <= 1.0, "4,0 no se puede pasar del blanco, salió {}", v[3]);
        assert!(v[3] > v[2], "dos luces distintas tienen que seguir siendo distintas");
        // Y con la exposición dentro del rango que la curva promete salvar
        // (ver `pf_image::color::doblar_luces`), nada se queda pegado al tope.
        assert!(v[2] < 1.0, "1,4 no debería llegar al blanco, salió {}", v[2]);
    }

    #[test]
    fn el_orden_mete_la_curva_entre_la_exposicion_y_el_contraste() {
        // Si la curva fuera después del contraste, subir el contraste podría
        // volver a sacar las luces del rango y quemarlas.
        assert!(Operacion::Exposicion { pasos: 1.0 }.orden() < Operacion::CurvaDeTono { activa: true }.orden());
        assert!(Operacion::CurvaDeTono { activa: true }.orden() < Operacion::Contraste { cantidad: 1.0 }.orden());
    }

    #[test]
    fn la_curva_apagada_es_neutra_y_por_eso_se_quita_sola() {
        assert!(Operacion::CurvaDeTono { activa: false }.es_neutra());
        assert!(!Operacion::CurvaDeTono { activa: true }.es_neutra());
        let partida = imagen(&[0.1, 3.0]);
        let mut img = partida.clone();
        aplicar(&[Operacion::CurvaDeTono { activa: false }], &mut img);
        assert_eq!(img.datos(), partida.datos());
    }


    /// Todos los controles tonales, en forma de función escalar sobre el
    /// valor perceptual. Para las pruebas de propiedades.
    fn tonal(p: f32, cual: &str, t: f32) -> f32 {
        match cual {
            "sombras" => sombras(p, t),
            "luces" => luces(p, t),
            "negros" => negros(p, t),
            "blancos" => blancos(p, t),
            _ => unreachable!(),
        }
    }

    const TONALES: [&str; 4] = ["sombras", "luces", "negros", "blancos"];

    #[test]
    fn los_cuatro_tonales_son_crecientes_en_todo_el_rango() {
        // Si alguno dejara de crecer, dos tonos distintos saldrían iguales o
        // —peor— del revés, y eso en una foto se ve como una mancha.
        for cual in TONALES {
            for t in [-1.0f32, -0.5, 0.5, 1.0] {
                let mut anterior = f32::MIN;
                for i in -2000..=30_000 {
                    let p = i as f32 / 10_000.0;
                    let y = tonal(p, cual, t);
                    assert!(
                        y > anterior,
                        "{cual} con t={t} dejó de crecer en p={p} ({y} tras {anterior})"
                    );
                    anterior = y;
                }
            }
        }
    }

    #[test]
    fn luces_y_sombras_no_mueven_el_blanco_ni_el_negro() {
        // Es la promesa que llevan en el tooltip, y por eso las máscaras
        // tienen pie y hombro. Negros y blancos sí los mueven: para eso están.
        for t in [-1.0f32, -0.5, 0.5, 1.0] {
            for cual in ["sombras", "luces"] {
                assert_eq!(tonal(0.0, cual, t), 0.0, "{cual} movió el negro");
                assert_eq!(tonal(1.0, cual, t), 1.0, "{cual} movió el blanco");
            }
        }
    }

    #[test]
    fn los_cuatro_dejan_los_medios_tonos_casi_quietos() {
        // En el pivote exacto las cuatro máscaras valen cero, y cerca de él
        // valen poco: si no, no serían controles de zona.
        for cual in TONALES {
            assert!((tonal(0.5, cual, 1.0) - 0.5).abs() < 1e-6, "{cual} movió el pivote");
            assert!((tonal(0.5, cual, -1.0) - 0.5).abs() < 1e-6);
        }
    }

    #[test]
    fn las_cifras_de_calibracion_son_las_que_dice_el_diseno() {
        // Números concretos, para que un retoque de las constantes no pase
        // desapercibido.
        let cerca = |a: f32, b: f32| assert!((a - b).abs() < 5e-4, "{a} debería ser {b}");
        cerca(blancos(0.9, 1.0), 1.0925);
        cerca(blancos(1.0, -1.0), 0.7407);
        cerca(negros(0.1, -1.0), -0.0925);
        cerca(negros(0.0, 1.0), 0.2593);
        // Y la sombra más oscura que se puede distinguir sigue siendo casi
        // negra después de subirla a tope: nada de escalones en el arranque.
        let un_nivel = 1.0 / 255.0;
        assert!(sombras(un_nivel, 1.0) * 255.0 < 4.0);
    }

    #[test]
    fn luces_y_sombras_no_sacan_la_luminancia_del_rango() {
        // El invariante se enuncia sobre la LUMINANCIA, que es donde es
        // cierto. En un color muy saturado, la ganancia sí puede llevar un
        // canal suelto por encima del blanco; eso lo fija la prueba de abajo.
        for a in [-1.0f32, -0.4, 0.4, 1.0] {
            for b in [-1.0f32, -0.4, 0.4, 1.0] {
                for i in 0..=1000 {
                    let p = i as f32 / 1000.0;
                    let y = luces(sombras(p, a), b);
                    assert!((-1e-6..=1.000001).contains(&y), "con {a}/{b} en {p} salió {y}");
                }
            }
        }
    }
    #[test]
    fn dentro_de_cada_par_el_orden_no_cambia_el_resultado() {
        // Es lo que permite que el panel elija su propio orden. Las máscaras
        // de luces y sombras no se solapan —una se anula donde empieza la
        // otra— y lo mismo pasa con blancos y negros.
        for i in 0..=1000 {
            let p = i as f32 / 1000.0;
            let a = luces(sombras(p, 0.7), -0.9);
            let b = sombras(luces(p, -0.9), 0.7);
            assert!((a - b).abs() < 1e-6, "luces/sombras no conmutan en {p}: {a} vs {b}");
            let c = blancos(negros(p, -0.6), 0.8);
            let d = negros(blancos(p, 0.8), -0.6);
            assert!((c - d).abs() < 1e-6, "blancos/negros no conmutan en {p}: {c} vs {d}");
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
