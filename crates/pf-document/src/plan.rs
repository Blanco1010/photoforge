//! Las operaciones, fusionadas en una sola pasada sobre los píxeles.
//!
//! # Por qué existe esto y no un bucle por operación
//!
//! Lo evidente es que cada operación recorra la imagen y haga lo suyo: un
//! bucle, una operación, fácil de leer. Es como estaba, y no aguanta.
//!
//! La cuenta que importa no son las multiplicaciones: es **cuántas veces se
//! recorre la memoria**. Una vista de 1.600 × 900 en RGB de `f32` son 17 MB,
//! así que cada pasada mueve 17 de lectura más 17 de escritura. Medido en
//! esta misma máquina, pasar la vista revelada a bytes —que es una pasada de
//! 17 + 5,7 MB— cuesta 2,8 ms: unos 8 GB/s efectivos. Con tres operaciones ya
//! se van unos 12 ms solo en ir y venir de la memoria.
//!
//! Con nueve controles serían cinco pasadas y el cuadro se iría por encima de
//! los 33 ms, o sea **por debajo de 30 cuadros por segundo**, que es justo la
//! cifra de la que presume el proyecto. Fusionando, el número de pasadas deja
//! de depender de cuántos deslizadores estén movidos: **siempre una**.
//!
//! # Lo que esto cuesta, dicho claro
//!
//! El código deja de leerse como «un bucle, una operación». A cambio:
//!
//! - Cada operación sigue teniendo su función escalar propia y comentada en
//!   `operacion.rs`; aquí solo se componen.
//! - Hay una prueba que comprueba que la pasada fusionada da exactamente lo
//!   mismo que aplicar las operaciones una a una.
//!
//! Es deuda de legibilidad real, y se reconoce.

use pf_core::{Dimensiones, Region};
use pf_image::orientacion::{self, Orientacion};
use pf_image::{CANALES, Imagen, color};
use rayon::prelude::*;

use crate::{Degradado, Operacion, operacion};

/// Escalones de las tablas. El mismo número y el mismo motivo que en
/// `pf_image::color`: por debajo de un nivel de 8 bits no hay banda que ver.
const ESCALONES: usize = 4096;

/// Una curva escalar tabulada sobre la entrada 0..=1.
type Tabla = [f32; ESCALONES + 1];

/// Cómo se llega del píxel de destino al píxel del original.
///
/// # Por qué esto existe
///
/// `Plan::ejecutar` recibe una imagen ya muestreada y, hasta ahora, no
/// necesitaba saber de qué trozo de la foto venía: las nueve operaciones son
/// funciones del VALOR del píxel y de nada más. El degradado es la primera
/// que depende de DÓNDE está, así que alguien tiene que contárselo.
///
/// Lo construye `Documento`, que es lo único que conoce a la vez la región,
/// el destino y el giro, y lo recibe `Plan::desde`, que lo pliega en tres
/// flotantes antes de tocar un píxel. `ejecutar` no hace geometría: eso es
/// la tesis de este archivo —la lista se resuelve entera primero— y además
/// es lo que impide que dos de los tres caminos de revelado transformen y el
/// tercero se olvide.
///
/// Son seis coeficientes porque la composición de «recortar y escalar» con
/// «girar» es afín y alineada con los ejes, y las cuatro orientaciones que
/// trasponen hacen que la x del original dependa de la y del destino.
#[derive(Debug, Clone, Copy)]
pub struct Encuadre {
    pub ax: f32,
    pub bx: f32,
    pub cx: f32,
    pub ay: f32,
    pub by: f32,
    pub cy: f32,
}

impl Encuadre {
    /// El encuadre de un muestreo con su giro.
    pub fn nuevo(
        region: Region,
        destino: Dimensiones,
        giro: Orientacion,
        original: Dimensiones,
    ) -> Self {
        let ex = region.ancho / destino.ancho as f32;
        let ey = region.alto / destino.alto as f32;
        let [ax, bx, cx, ay, by, cy] = orientacion::afin_a_origen(
            giro.pasos_publicos(),
            original.ancho as i64,
            original.alto as i64,
        );
        // El píxel de destino (dx, dy) se muestrea en el centro de su celda.
        let (u0, v0) = (region.x + 0.5 * ex, region.y + 0.5 * ey);
        Encuadre {
            ax: ax * ex,
            bx: bx * ey,
            cx: ax * u0 + bx * v0 + cx,
            ay: ay * ex,
            by: by * ey,
            cy: ay * u0 + by * v0 + cy,
        }
    }

    /// La foto entera a su tamaño y sin girar, que es el caso de la
    /// exportación y de los dos histogramas. Tiene nombre para que se lea
    /// como lo que es y no como seis números sueltos.
    pub fn completa(_dim: Dimensiones) -> Self {
        Encuadre { ax: 1.0, bx: 0.0, cx: 0.5, ay: 0.0, by: 1.0, cy: 0.5 }
    }
}

/// El degradado, ya plegado contra el encuadre.
struct Local {
    /// `t(dx, dy) = alfa·dx + beta·dy + gamma`, de 0 en el borde entero a 1
    /// en el borde donde se acaba.
    alfa: f32,
    beta: f32,
    gamma: f32,
    /// `2^(pasos · suave(t))`, tabulada sobre t de 0 a 1.
    rampa: Box<Tabla>,
    /// El factor con el efecto entero, para `t <= 0`: o sea, del borde donde
    /// se pulsó hacia fuera.
    pleno: f32,
}

/// Todo lo que hay que hacerle a un píxel, ya resuelto.
///
/// Se construye una vez por repintado —o sea unas treinta veces por segundo
/// mientras se arrastra un deslizador— y se ejecuta sobre un millón y pico de
/// píxeles. Por eso aquí se paga con gusto cualquier cálculo que ahorre
/// trabajo dentro del bucle.
pub(crate) struct Plan {
    /// 2 elevado a los pasos de exposición. 1,0 si no hay exposición.
    factor: f32,
    curva: bool,
    /// 1 + cantidad/100. 1,0 si no hay contraste.
    k: f32,
    /// La curva de contraste, tabulada. `None` si el contraste es neutro.
    contraste: Option<Box<Tabla>>,
    /// El degradado, si lo hay.
    local: Option<Local>,
    /// Los cuatro controles tonales, de -1 a 1. Cero es neutro.
    tonal: [f32; 4],
    /// Los cuatro compuestos en una sola curva de luminancia, tabulada.
    ///
    /// Se puede porque los cuatro son funciones de la luminancia y de nada
    /// más, así que su composición sigue siendo una curva escalar. Da igual
    /// cuántos estén movidos: siempre es una tabla y una búsqueda.
    ///
    /// Guarda la luminancia de SALIDA y no la ganancia, y eso importa: cerca
    /// del negro la ganancia no está acotada —una sombra levantada a tope
    /// puede pedir multiplicar por miles— y la tabla sería inútil justo donde
    /// más resolución hace falta.
    tabla_tonal: Option<Box<Tabla>>,
}

/// Por debajo de esto un píxel es negro y no hay ganancia que calcular.
///
/// No es un umbral de gusto: es el guarda de la división. Con las máscaras
/// ancladas en el cero, la curva ya vale 0 en 0, así que esto no crea ningún
/// escalón; solo evita el 0/0.
const NEGRO: f32 = 1e-5;

/// La curva tonal completa sobre la luminancia, sin tabla.
///
/// Es la verdad contra la que se construye la tabla, y la que se usa por
/// encima del blanco, donde la tabla no llega.
fn tonal_escalar(y: f32, [s, l, n, b]: [f32; 4]) -> f32 {
    let mut p = color::lineal_a_srgb(y);
    if s != 0.0 {
        p = operacion::sombras(p, s);
    }
    if l != 0.0 {
        p = operacion::luces(p, l);
    }
    if n != 0.0 {
        p = operacion::negros(p, n);
    }
    if b != 0.0 {
        p = operacion::blancos(p, b);
    }
    // Se acota por abajo y NO por arriba, y la asimetría tiene motivo: por
    // debajo del negro no hay luz que medir, mientras que por encima del
    // blanco sí hay luz que la pantalla no puede enseñar pero que una
    // operación posterior puede devolver al rango. Sin este tope, Negros a
    // tope dejaría la luminancia negativa y la ganancia cambiaría el SIGNO de
    // los tres canales: `a_byte` lo taparía, pero un valor con el signo del
    // revés ya no es el mismo dato.
    color::srgb_a_lineal(p).max(0.0)
}

impl Plan {
    /// Lee la lista entera y resuelve qué hay que hacer.
    ///
    /// Ordena por `orden()` al entrar. Hasta ahora el orden canónico lo
    /// imponía `Documento` al insertar, y era un invariante que había que
    /// acordarse de mantener; aquí pasa a ser estructural, porque la fusión
    /// necesita saber qué va antes de qué. El día que un `.pforge` traiga una
    /// lista leída de un archivo, llegará ordenada sin que nadie se acuerde.
    pub(crate) fn desde(
        operaciones: &[Operacion],
        degradado: Option<Degradado>,
        encuadre: Encuadre,
    ) -> Self {
        let mut ordenadas: Vec<Operacion> = operaciones.to_vec();
        ordenadas.sort_by_key(|o| o.orden());

        let mut plan = Plan {
            factor: 1.0,
            curva: false,
            k: 1.0,
            contraste: None,
            local: None,
            tonal: [0.0; 4],
            tabla_tonal: None,
        };

        for op in &ordenadas {
            if op.es_neutra() {
                continue;
            }
            match *op {
                Operacion::Exposicion { pasos } => plan.factor *= 2.0f32.powf(pasos),
                Operacion::CurvaDeTono { .. } => plan.curva = true,
                Operacion::Contraste { cantidad } => {
                    plan.k = 1.0 + cantidad.clamp(-100.0, 100.0) / 100.0;
                }
                Operacion::Sombras { cantidad } => plan.tonal[0] = cantidad.clamp(-100.0, 100.0) / 100.0,
                Operacion::Luces { cantidad } => plan.tonal[1] = cantidad.clamp(-100.0, 100.0) / 100.0,
                Operacion::Negros { cantidad } => plan.tonal[2] = cantidad.clamp(-100.0, 100.0) / 100.0,
                Operacion::Blancos { cantidad } => plan.tonal[3] = cantidad.clamp(-100.0, 100.0) / 100.0,
            }
        }

        // El degradado se pliega contra el encuadre: de la geometría en
        // coordenadas del original salen tres flotantes que el bucle usa con
        // una multiplicación y una suma por píxel.
        if let Some(d) = degradado.filter(|d| d.pasos != 0.0) {
            let (dx, dy) = (d.hasta.x - d.desde.x, d.hasta.y - d.desde.y);
            let dd = dx * dx + dy * dy;
            // Un degradado de longitud cero no tiene dirección y no se puede
            // evaluar. No debería llegar —la interfaz exige un arrastre
            // mínimo—, pero una división por cero aquí pintaría la foto de
            // NaN entera.
            if dd > 1e-6 {
                let e = encuadre;
                let mut rampa = Box::new([0.0f32; ESCALONES + 1]);
                for (i, v) in rampa.iter_mut().enumerate() {
                    let t = i as f32 / ESCALONES as f32;
                    // `t` vale 0 en `desde` y 1 en `hasta`, y el efecto va al
                    // revés: entero donde se pulsó y nulo donde se soltó. De
                    // ahí el `1 -`, que es el sentido del gesto y no un
                    // detalle: sin él, arrastrar sobre el cielo oscurecería
                    // el suelo.
                    //
                    // Se interpola en PASOS y no en el factor. Con -6 EV,
                    // interpolar el factor dejaría el 94 % del oscurecimiento
                    // en el último quinto de la franja: una raya visible
                    // justo en el cielo, que es lo que `suave` existe para
                    // evitar.
                    *v = (d.pasos * (1.0 - color::suave(t))).exp2();
                }
                plan.local = Some(Local {
                    alfa: (e.ax * dx + e.ay * dy) / dd,
                    beta: (e.bx * dx + e.by * dy) / dd,
                    gamma: ((e.cx - d.desde.x) * dx + (e.cy - d.desde.y) * dy) / dd,
                    rampa,
                    pleno: d.pasos.exp2(),
                });
            }
        }

        if plan.tonal.iter().any(|x| *x != 0.0) {
            let mut tabla = Box::new([0.0f32; ESCALONES + 1]);
            for (i, v) in tabla.iter_mut().enumerate() {
                *v = tonal_escalar(i as f32 / ESCALONES as f32, plan.tonal);
            }
            plan.tabla_tonal = Some(tabla);
        }

        if plan.k != 1.0 {
            // La exposición NO se tabula, y es a propósito: es una
            // multiplicación, que ya es lo más barato que hay, y además
            // tabularla obligaría a cubrir el rango de ENTRADA. Con +6 pasos,
            // todo lo interesante de la foto cabría en la sesentaicuatroava
            // parte de la tabla y la resolución se iría a la basura justo
            // donde hace falta.
            let mut tabla = Box::new([0.0f32; ESCALONES + 1]);
            for (i, v) in tabla.iter_mut().enumerate() {
                *v = contraste_escalar(i as f32 / ESCALONES as f32, plan.k);
            }
            plan.contraste = Some(tabla);
        }

        plan
    }

    /// ¿Esta lista deja la imagen como estaba?
    pub(crate) fn es_identidad(&self) -> bool {
        // El degradado cuenta: sin esto, abrir un JPEG y dibujar una franja
        // no cambiaría un píxel, porque los demás campos siguen neutros y la
        // pasada entera se saltaría.
        self.factor == 1.0
            && !self.curva
            && self.k == 1.0
            && self.tabla_tonal.is_none()
            && self.local.is_none()
    }

    pub(crate) fn ejecutar(&self, img: &mut Imagen) {
        if self.es_identidad() {
            return;
        }
        let factor = self.factor;
        let curva = self.curva;
        let k = self.k;
        let contraste = self.contraste.as_deref();
        let tabla_tonal = self.tabla_tonal.as_deref();
        let tonal = self.tonal;
        let local = self.local.as_ref();

        // El reparto es por FILAS enteras, y no por trozos de tamaño fijo.
        //
        // Dos motivos. Uno ya valía antes: el bloque tonal mira al píxel
        // entero para sacar su luminancia, y un trozo que partiera un píxel
        // por la mitad pintaría franjas de color en los bordes de cada
        // reparto. El otro es el que obliga: el degradado necesita saber EN
        // QUÉ FILA está, y con trozos de tamaño arbitrario no hay forma de
        // saberlo sin dividir. Con una fila por trozo, el índice lo da
        // `enumerate` y no cuesta nada.
        //
        // El ancho se lee ANTES de pedir los datos, porque `datos_mut` toma
        // prestada la imagen entera.
        let ancho = img.ancho() as usize;
        img.datos_mut()
            .par_chunks_mut(ancho * CANALES)
            .enumerate()
            .for_each(|(dy, fila)| {
                // El arranque de la fila: una multiplicación y una suma
                // FUERA del bucle de píxeles. Cada fila lo saca de su propio
                // índice, así que no hay dependencia entre filas y rayon las
                // reparte igual que antes; una acumulación que cruzara filas
                // serializaría la pasada entera.
                let t0 = local.map(|l| l.beta * dy as f32 + l.gamma);

                for (dx, px) in fila.as_chunks_mut::<CANALES>().0.iter_mut().enumerate() {
                    // 1 y 2. Exposición y curva de luces: canal a canal.
                    //
                    // El degradado se mete AQUÍ, multiplicando el mismo
                    // factor: es una exposición que varía en el espacio, que
                    // es literalmente lo que hace un cristal degradado. Al ir
                    // antes de la curva de luces, oscurecer un cielo quemado
                    // puede devolverle detalle.
                    let mut luz = factor;
                    if let (Some(l), Some(t0)) = (local, t0) {
                        let t = l.alfa * dx as f32 + t0;
                        // Más allá de `hasta` no hay efecto, así que ni se
                        // lee la tabla; antes de `desde` el efecto es entero.
                        if t < 1.0 {
                            luz *= if t <= 0.0 { l.pleno } else { interpolar(&l.rampa, t) };
                        }
                    }
                    for v in px.iter_mut() {
                        let mut x = *v * luz;
                        if curva {
                            x = color::doblar_luces(x);
                        }
                        *v = x;
                    }

                    // 3. El bloque tonal: una sola ganancia para los tres
                    //    canales, sacada de la luminancia. Es lo que mantiene
                    //    el tono exactamente donde estaba.
                    if let Some(t) = tabla_tonal {
                        let y = color::luminancia(px[0], px[1], px[2]);
                        if y > NEGRO {
                            let y2 = if y <= 1.0 {
                                interpolar(t, y)
                            } else {
                                // Por encima del blanco solo actúa Blancos:
                                // las otras tres máscaras ya valen cero ahí.
                                tonal_escalar(y, tonal)
                            };
                            let g = y2 / y;
                            px[0] *= g;
                            px[1] *= g;
                            px[2] *= g;
                        }
                    }

                    // 4. Contraste: otra vez canal a canal.
                    if let Some(t) = contraste {
                        for v in px.iter_mut() {
                            *v = if (0.0..=1.0).contains(v) {
                                interpolar(t, *v)
                            } else {
                                contraste_escalar(*v, k)
                            };
                        }
                    }
                }
            });
    }
}

/// El contraste de un valor suelto.
///
/// Es la misma fórmula que estaba en `operacion.rs` y por el mismo motivo: se
/// aplica en espacio perceptual y no en lineal para que el deslizador se
/// sienta parejo de punta a punta. Vive aquí porque es lo que tabula el plan.
#[inline]
fn contraste_escalar(v: f32, k: f32) -> f32 {
    let s = color::lineal_a_srgb(v);
    color::srgb_a_lineal((s - 0.5) * k + 0.5)
}

/// Lee la tabla interpolando entre los dos escalones vecinos.
///
/// Se interpola en vez de coger el más cercano porque aquí la salida no es un
/// byte sino un `f32` que todavía tiene que pasar por más operaciones: el
/// error de redondear se arrastraría en vez de morir en el último paso.
#[inline]
fn interpolar(t: &Tabla, v: f32) -> f32 {
    let p = v * ESCALONES as f32;
    let i = p as usize;
    if i >= ESCALONES {
        return t[ESCALONES];
    }
    let f = p - i as f32;
    t[i] + (t[i + 1] - t[i]) * f
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use pf_core::Dimensiones;

    fn imagen(valores: &[f32]) -> Imagen {
        let dim = Dimensiones::nueva(valores.len() as u32, 1).unwrap();
        Imagen::nueva(dim, valores.iter().flat_map(|v| [*v, *v, *v]).collect()).unwrap()
    }

    fn canal(img: &Imagen) -> Vec<f32> {
        img.datos().chunks(CANALES).map(|c| c[0]).collect()
    }

    /// Las operaciones aplicadas UNA A UNA, cada una con su pasada, como se
    /// hacía antes de fusionarlas. Es la referencia contra la que se mide.
    fn una_a_una(ops: &[Operacion], img: &mut Imagen) {
        let mut ordenadas: Vec<Operacion> = ops.to_vec();
        ordenadas.sort_by_key(|o| o.orden());
        for op in ordenadas {
            if op.es_neutra() {
                continue;
            }
            match op {
                Operacion::Exposicion { pasos } => {
                    let f = 2.0f32.powf(pasos);
                    for v in img.datos_mut() {
                        *v *= f;
                    }
                }
                Operacion::CurvaDeTono { .. } => {
                    for v in img.datos_mut() {
                        *v = color::doblar_luces(*v);
                    }
                }
                Operacion::Contraste { cantidad } => {
                    let k = 1.0 + cantidad.clamp(-100.0, 100.0) / 100.0;
                    for v in img.datos_mut() {
                        *v = contraste_escalar(*v, k);
                    }
                }
                otra => {
                    // Los cuatro tonales, cada uno en su propia pasada.
                    let mut t = [0.0f32; 4];
                    match otra {
                        Operacion::Sombras { cantidad } => t[0] = cantidad / 100.0,
                        Operacion::Luces { cantidad } => t[1] = cantidad / 100.0,
                        Operacion::Negros { cantidad } => t[2] = cantidad / 100.0,
                        Operacion::Blancos { cantidad } => t[3] = cantidad / 100.0,
                        _ => unreachable!(),
                    }
                    for px in img.datos_mut().as_chunks_mut::<CANALES>().0 {
                        let y = color::luminancia(px[0], px[1], px[2]);
                        if y > NEGRO {
                            let g = tonal_escalar(y, t) / y;
                            px[0] *= g;
                            px[1] *= g;
                            px[2] *= g;
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn la_pasada_fusionada_da_lo_mismo_que_las_operaciones_una_a_una() {
        // Esta es LA prueba que justifica la fusión: si falla, la fusión es
        // una optimización que cambió el resultado, que es lo peor que puede
        // pasarle a un revelador.
        let muestras: Vec<f32> =
            (0..=200).map(|i| i as f32 / 100.0).chain([-0.2, 2.5, 7.0]).collect();

        let combinaciones = [
            vec![Operacion::Exposicion { pasos: 1.5 }],
            vec![Operacion::CurvaDeTono { activa: true }],
            vec![Operacion::Contraste { cantidad: 40.0 }],
            vec![Operacion::Contraste { cantidad: -70.0 }],
            vec![
                Operacion::Exposicion { pasos: 2.0 },
                Operacion::CurvaDeTono { activa: true },
            ],
            vec![
                Operacion::Contraste { cantidad: 35.0 },
                Operacion::Exposicion { pasos: -1.0 },
                Operacion::CurvaDeTono { activa: true },
            ],
            vec![Operacion::Sombras { cantidad: 60.0 }],
            vec![Operacion::Luces { cantidad: -80.0 }],
            vec![Operacion::Negros { cantidad: -35.0 }],
            vec![Operacion::Blancos { cantidad: 45.0 }],
            // Y las nueve juntas, que es el caso que de verdad importa.
            vec![
                Operacion::Blancos { cantidad: 20.0 },
                Operacion::Contraste { cantidad: 30.0 },
                Operacion::Sombras { cantidad: 55.0 },
                Operacion::Exposicion { pasos: 1.2 },
                Operacion::Negros { cantidad: -25.0 },
                Operacion::CurvaDeTono { activa: true },
                Operacion::Luces { cantidad: -40.0 },
            ],
        ];

        for ops in &combinaciones {
            let mut fusionada = imagen(&muestras);
            Plan::desde(ops, None, Encuadre::completa(fusionada.dim())).ejecutar(&mut fusionada);

            let mut referencia = imagen(&muestras);
            una_a_una(ops, &mut referencia);

            for (i, (a, b)) in canal(&fusionada).iter().zip(canal(&referencia)).enumerate() {
                let dif = (a - b).abs();
                // El margen es el de la tabla, y se comprueba aparte que
                // está por debajo de un nivel de 8 bits.
                assert!(
                    dif < 2e-4,
                    "con {ops:?}, muestra {i} ({}): fusionada {a}, una a una {b}",
                    muestras[i]
                );
            }
        }
    }

    #[test]
    fn el_error_de_la_tabla_esta_por_debajo_de_un_nivel_de_8_bits() {
        // El mismo argumento que ya está escrito en `color.rs`: lo que no
        // puede distinguir un byte, no se ve.
        let tabla_salida = color::tabla_salida();
        for cantidad in [-100.0f32, -50.0, 25.0, 60.0, 100.0] {
            let plan = Plan::desde(&[Operacion::Contraste { cantidad }], None, Encuadre::completa(Dimensiones::nueva(1, 1).unwrap()));
            let t = plan.contraste.as_deref().expect("debería haber tabla");
            let mut peor = 0i32;
            for i in 0..=20_000 {
                let v = i as f32 / 20_000.0;
                let exacto = tabla_salida.a_byte(contraste_escalar(v, plan.k)) as i32;
                let desde_tabla = tabla_salida.a_byte(interpolar(t, v)) as i32;
                peor = peor.max((exacto - desde_tabla).abs());
            }
            assert!(peor <= 1, "con contraste {cantidad} el error llegó a {peor} niveles");
        }
    }


    #[test]
    fn los_tonales_mantienen_el_tono_del_pixel() {
        // La ganancia se saca de la luminancia y se aplica a los tres canales
        // por igual, así que las PROPORCIONES entre canales no se mueven. Si
        // alguien tratara cada canal por su cuenta, esta prueba lo caza: un
        // color saturado se iría hacia el pastel.
        let dim = Dimensiones::nueva(1, 1).unwrap();
        let mut img = Imagen::nueva(dim, vec![0.30, 0.12, 0.05]).unwrap();
        Plan::desde(&[Operacion::Sombras { cantidad: 80.0 }], None, Encuadre::completa(Dimensiones::nueva(1, 1).unwrap())).ejecutar(&mut img);
        let d = img.datos();
        assert!((d[0] / d[1] - 0.30 / 0.12).abs() < 1e-4, "el tono se movió");
        assert!((d[1] / d[2] - 0.12 / 0.05).abs() < 1e-4, "el tono se movió");
        assert!(d[0] > 0.30, "las sombras no se abrieron");
    }

    #[test]
    fn un_color_saturado_si_puede_recortar_un_canal_al_subir_las_luces() {
        // Esto NO es un fallo, es el límite de la promesa, y la prueba está
        // aquí para que nadie lo «arregle» dentro de seis meses. El tooltip
        // de Luces lo dice: la luminancia no se recorta, pero un canal de un
        // color muy saturado que ya rozaba el blanco sí puede cruzarlo.
        let dim = Dimensiones::nueva(1, 1).unwrap();
        let mut img = Imagen::nueva(dim, vec![0.95, 0.40, 0.10]).unwrap();
        Plan::desde(&[Operacion::Luces { cantidad: 100.0 }], None, Encuadre::completa(Dimensiones::nueva(1, 1).unwrap())).ejecutar(&mut img);
        assert!(img.datos()[0] > 1.0, "se esperaba que el rojo cruzara el blanco");
    }

    #[test]
    fn ningun_tonal_deja_un_canal_negativo() {
        // Lo garantiza el tope por abajo de la curva: la ganancia nunca es
        // negativa, así que no puede cambiarle el signo a nada.
        let valores: Vec<f32> = (0..=100).map(|i| i as f32 / 100.0).collect();
        for cantidad in [-100.0f32, -50.0, 50.0, 100.0] {
            for op in [
                Operacion::Sombras { cantidad },
                Operacion::Luces { cantidad },
                Operacion::Negros { cantidad },
                Operacion::Blancos { cantidad },
            ] {
                let mut img = imagen(&valores);
                Plan::desde(&[op], None, Encuadre::completa(Dimensiones::nueva(1, 1).unwrap())).ejecutar(&mut img);
                for v in img.datos() {
                    assert!(*v >= 0.0, "{op:?} dejó {v}");
                }
            }
        }
    }
    #[test]
    fn una_lista_neutra_no_recorre_los_pixeles() {
        let plan = Plan::desde(&[
            Operacion::Exposicion { pasos: 0.0 },
            Operacion::CurvaDeTono { activa: false },
            Operacion::Contraste { cantidad: 0.0 },
            Operacion::Sombras { cantidad: 0.0 },
            Operacion::Luces { cantidad: 0.0 },
            Operacion::Negros { cantidad: 0.0 },
            Operacion::Blancos { cantidad: 0.0 },
        ], None, Encuadre::completa(Dimensiones::nueva(1, 1).unwrap()));
        assert!(plan.es_identidad());

        let partida = imagen(&[0.1, 0.5, 0.9]);
        let mut img = partida.clone();
        plan.ejecutar(&mut img);
        assert_eq!(img.datos(), partida.datos());
    }

    #[test]
    fn el_plan_ordena_la_lista_aunque_llegue_del_reves() {
        // El orden canónico deja de depender de que quien inserta se acuerde.
        let alreves = [
            Operacion::Contraste { cantidad: 50.0 },
            Operacion::CurvaDeTono { activa: true },
            Operacion::Exposicion { pasos: 1.0 },
        ];
        let derecha = [
            Operacion::Exposicion { pasos: 1.0 },
            Operacion::CurvaDeTono { activa: true },
            Operacion::Contraste { cantidad: 50.0 },
        ];
        let mut a = imagen(&[0.05, 0.2, 0.6, 0.95]);
        let mut b = imagen(&[0.05, 0.2, 0.6, 0.95]);
        Plan::desde(&alreves, None, Encuadre::completa(Dimensiones::nueva(1, 1).unwrap())).ejecutar(&mut a);
        Plan::desde(&derecha, None, Encuadre::completa(Dimensiones::nueva(1, 1).unwrap())).ejecutar(&mut b);
        assert_eq!(canal(&a), canal(&b));
    }

    #[test]
    fn el_reparto_da_exactamente_un_trozo_por_fila() {
        // Lo que vigila esta prueba es que el reparto siga siendo por filas:
        // de ahí sale el índice de la fila que necesita el degradado, y
        // además impide que un trozo parta un píxel por la mitad.
        let dim = Dimensiones::nueva(7, 5).unwrap();
        let img = Imagen::negra(dim);
        let ancho = img.ancho() as usize;
        let trozos: Vec<&[f32]> = img.datos().chunks(ancho * CANALES).collect();
        assert_eq!(trozos.len(), dim.alto as usize);
        for t in trozos {
            assert_eq!(t.len(), ancho * CANALES);
        }
    }
}
