//! El documento no destructivo de PhotoForge.
//!
//! # Qué quiere decir «no destructivo», en una frase
//!
//! El archivo que el usuario abrió no se toca nunca, y los píxeles que se
//! cargaron de él tampoco. Un documento es **el original más una lista de
//! operaciones**, y la foto que se ve es el resultado de aplicar esa lista
//! sobre una copia. Deshacer no es recuperar píxeles guardados: es quitar un
//! elemento de una lista y volver a calcular.
//!
//! Eso tiene tres consecuencias que valen todo lo que cuesta:
//!
//! - Cambiar la exposición después de haber cambiado el contraste no acumula
//!   pérdida, porque las dos se aplican siempre sobre el original limpio.
//! - El historial ocupa lo que ocupan dos `f32`, no lo que ocupa una foto.
//! - Guardar un proyecto es guardar la lista, no los píxeles.
//!
//! Y una que cuesta: cada cambio obliga a recalcular. Por eso el cálculo se
//! hace sobre el trozo que se ve y no sobre la foto entera —ver
//! `pf_image::muestreo`—, y por eso la exportación es el único sitio donde se
//! paga el precio completo.

pub mod operacion;

use std::path::{Path, PathBuf};

use pf_core::{Dimensiones, Procedencia, Region, Resultado};
use pf_image::{Imagen, histograma::Histograma, metadatos::Metadatos};

pub use operacion::{Clase, Operacion};

/// Qué versión de la foto se quiere.
///
/// Existe por el botón de «Antes». Lo evidente sería que «antes» fuera la
/// foto sin ninguna operación, y para un JPEG lo es. Para un RAW no: sin
/// operaciones, un RAW son fotones medidos y se ve casi negro, así que
/// comparar contra eso no enseña lo que acabas de hacer, enseña lo que hace
/// un RAW sin revelar.
///
/// Así que «antes» es **como se abrió la foto**, con sus ajustes de partida.
/// Es lo mismo contra lo que vuelve «Restablecer», y las dos cosas tienen que
/// significar lo mismo o el programa se contradice a sí mismo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Revelado {
    /// Con los ajustes que hay ahora mismo.
    Actual,
    /// Con los ajustes con los que se abrió.
    ComoSeAbrio,
}

/// Cuántos pasos atrás se guardan.
///
/// Un paso es una lista de dos o tres operaciones, o sea unas decenas de
/// bytes: cien pasos no llegan a un kilobyte. El tope no está por memoria,
/// está para que la pila no crezca sin fin en una sesión larga.
const PASOS_DE_HISTORIAL: usize = 100;

/// Una foto abierta, con todo lo que PhotoForge sabe de ella.
pub struct Documento {
    ruta: PathBuf,
    /// Los píxeles tal como salieron del archivo. Esto no se modifica jamás;
    /// si alguna vez hace falta un `&mut` sobre esto, algo se ha torcido.
    original: Imagen,
    metadatos: Metadatos,
    procedencia: Procedencia,
    /// Lo que la cámara dice de sí misma en un RAW. Un JPEG trae esto en el
    /// EXIF; un RAW también, pero el decodificador lo da ya limpio y en un
    /// formato comparable entre marcas.
    camara_del_raw: Option<String>,
    con_mosaico: bool,
    operaciones: Vec<Operacion>,
    /// Los ajustes con los que nació el documento.
    ///
    /// Un RAW no se abre con la lista vacía: se abre con una exposición de
    /// partida y la curva de luces, porque sin eso se vería oscuro. Guardar
    /// aquí ese punto de partida es lo que permite que «Restablecer» devuelva
    /// la foto a como se abrió y no a unos datos crudos que parecerían rotos.
    iniciales: Vec<Operacion>,
    deshacer: Vec<Vec<Operacion>>,
    rehacer: Vec<Vec<Operacion>>,
}

impl Documento {
    /// Abre un archivo.
    ///
    /// Un RAW no sale de aquí con la lista de operaciones vacía. Sus números
    /// son fotones medidos y enseñarlos tal cual da una foto oscura, así que
    /// se le ponen dos ajustes de partida: la exposición que calcula
    /// [`pf_image::raw::exposicion_sugerida`] y la curva de luces.
    ///
    /// Los dos van en la lista normal, se ven en sus controles y se pueden
    /// quitar. PhotoForge no tiene ajustes escondidos: lo que cambia la foto
    /// está siempre a la vista.
    pub fn abrir(ruta: &Path) -> Resultado<Self> {
        let cargada = pf_image::carga::cargar(ruta)?;
        let metadatos = pf_image::metadatos::leer(ruta);

        let iniciales = if cargada.procedencia.es_escena() {
            let mut v = vec![Operacion::CurvaDeTono { activa: true }];
            let pasos = pf_image::raw::exposicion_sugerida(&cargada.imagen);
            if pasos != 0.0 {
                v.push(Operacion::Exposicion { pasos });
            }
            v.sort_by_key(|o| o.orden());
            v
        } else {
            Vec::new()
        };

        Ok(Self {
            ruta: ruta.to_path_buf(),
            original: cargada.imagen,
            metadatos,
            procedencia: cargada.procedencia,
            camara_del_raw: cargada.camara,
            con_mosaico: cargada.con_mosaico,
            operaciones: iniciales.clone(),
            iniciales,
            deshacer: Vec::new(),
            rehacer: Vec::new(),
        })
    }

    pub fn procedencia(&self) -> Procedencia {
        self.procedencia
    }

    pub fn camara_del_raw(&self) -> Option<&str> {
        self.camara_del_raw.as_deref()
    }

    pub fn con_mosaico(&self) -> bool {
        self.con_mosaico
    }

    pub fn ruta(&self) -> &Path {
        &self.ruta
    }

    pub fn dim(&self) -> Dimensiones {
        self.original.dim()
    }

    pub fn metadatos(&self) -> &Metadatos {
        &self.metadatos
    }

    pub fn operaciones(&self) -> &[Operacion] {
        &self.operaciones
    }

    pub fn bytes_en_memoria(&self) -> usize {
        self.original.bytes()
    }

    /// ¿Se ha tocado algo desde que se abrió?
    ///
    /// Se compara con los ajustes de partida y no con la lista vacía: un RAW
    /// recién abierto YA tiene exposición y curva, y decir que «hay cambios»
    /// nada más abrirlo sería mentir y encendería el botón de comparar sin
    /// que haya nada que comparar.
    pub fn hay_cambios(&self) -> bool {
        self.operaciones != self.iniciales
    }

    /// Los ajustes con los que se abrió, que es a donde vuelve «Restablecer».
    pub fn iniciales(&self) -> &[Operacion] {
        &self.iniciales
    }

    pub fn puede_deshacer(&self) -> bool {
        !self.deshacer.is_empty()
    }

    pub fn puede_rehacer(&self) -> bool {
        !self.rehacer.is_empty()
    }

    /// Pone o cambia una operación.
    ///
    /// Si ya había una de la misma clase, la sustituye en vez de añadir otra:
    /// mover el deslizador de exposición treinta veces tiene que dejar UNA
    /// exposición en la lista, no treinta encadenadas. Y si la operación
    /// resultante es neutra, se quita del todo, para que un deslizador
    /// devuelto a cero deje el documento literalmente como estaba.
    ///
    /// Devuelve `false` si no cambió nada, para que quien llame pueda
    /// ahorrarse el repintado.
    pub fn ajustar(&mut self, op: Operacion) -> bool {
        let anterior = self.operaciones.clone();

        self.operaciones.retain(|o| o.clase() != op.clase());
        if !op.es_neutra() {
            self.operaciones.push(op);
            // El orden de aplicación es el canónico y no el de uso. El porqué
            // está en `Operacion::orden`.
            self.operaciones.sort_by_key(|o| o.orden());
        }

        if self.operaciones == anterior {
            return false;
        }
        self.apuntar(anterior);
        true
    }

    /// Devuelve la foto a como se abrió.
    ///
    /// Para un JPEG eso es la lista vacía. Para un RAW es su exposición de
    /// partida y su curva: dejarlo en crudo enseñaría una foto negra y
    /// parecería que el botón rompió algo.
    pub fn restablecer(&mut self) -> bool {
        if self.operaciones == self.iniciales {
            return false;
        }
        let anterior = std::mem::replace(&mut self.operaciones, self.iniciales.clone());
        self.apuntar(anterior);
        true
    }

    pub fn deshacer(&mut self) -> bool {
        match self.deshacer.pop() {
            Some(estado) => {
                self.rehacer.push(std::mem::replace(&mut self.operaciones, estado));
                true
            }
            None => false,
        }
    }

    pub fn rehacer(&mut self) -> bool {
        match self.rehacer.pop() {
            Some(estado) => {
                self.deshacer.push(std::mem::replace(&mut self.operaciones, estado));
                true
            }
            None => false,
        }
    }

    /// Guarda un estado en la pila de deshacer.
    ///
    /// Apuntar BORRA la pila de rehacer, y eso es intencionado: si alguien
    /// deshace tres pasos y luego hace algo distinto, los tres pasos que había
    /// delante ya no son el futuro de este documento. Conservarlos daría un
    /// «rehacer» que salta a una historia que no ocurrió.
    fn apuntar(&mut self, estado: Vec<Operacion>) {
        self.deshacer.push(estado);
        if self.deshacer.len() > PASOS_DE_HISTORIAL {
            self.deshacer.remove(0);
        }
        self.rehacer.clear();
    }

    /// Los píxeles del original, sin tocar. Para comparar «antes y después».
    pub fn original(&self) -> &Imagen {
        &self.original
    }

    /// Calcula lo que hay que pintar en el lienzo.
    ///
    /// Primero se recorta y se escala, después se revela. El orden importa
    /// mucho: al revés habría que revelar la foto entera para tirar el 97 %.
    ///
    /// Los dos revelados se muestrean EXACTAMENTE igual —misma región, mismo
    /// tamaño, mismo filtro—, para que la comparación enseñe la diferencia
    /// del revelado y no la del reescalado.
    pub fn vista(&self, region: Region, destino: Dimensiones, que: Revelado) -> Imagen {
        let mut img = pf_image::muestreo::muestrear(&self.original, region, destino);
        operacion::aplicar(self.lista(que), &mut img);
        img
    }

    fn lista(&self, que: Revelado) -> &[Operacion] {
        match que {
            Revelado::Actual => &self.operaciones,
            Revelado::ComoSeAbrio => &self.iniciales,
        }
    }

    /// La foto entera revelada, a resolución completa. Para exportar.
    ///
    /// Este es el único sitio donde se paga el coste completo, y se paga una
    /// vez por exportación.
    pub fn completa(&self, que: Revelado) -> Imagen {
        let mut img = self.original.clone();
        operacion::aplicar(self.lista(que), &mut img);
        img
    }

    /// El histograma de lo que se vería.
    ///
    /// Se calcula sobre la foto COMPLETA revelada y no sobre la vista, y esa
    /// diferencia con el lienzo es deliberada: el histograma sirve para saber
    /// si hay luces quemadas en la foto, y si se calculara sobre el trozo
    /// visible cambiaría al hacer zoom, que es justo lo que no debe hacer.
    ///
    /// El coste se mantiene a raya porque `histograma::calcular` muestrea un
    /// millón de píxeles como mucho; lo caro aquí es aplicar las operaciones
    /// sobre la foto entera. Por eso existe `histograma_rapido`.
    pub fn histograma(&self) -> Histograma {
        pf_image::histograma::calcular(&self.completa(Revelado::Actual))
    }

    /// El histograma calculado sobre una versión reducida de la foto.
    ///
    /// Es el que usa la interfaz mientras se arrastra un deslizador. La forma
    /// de la curva es indistinguible de la del completo —son los mismos tonos
    /// en las mismas proporciones—, pero cuesta lo que cuesta revelar dos
    /// megapíxeles en vez de cincuenta.
    ///
    /// Los recuentos de recorte SÍ cambian: en una versión reducida, un grupo
    /// de cuatro píxeles quemados puede promediarse con sus vecinos y dejar de
    /// estarlo. Por eso la interfaz enseña el aviso de recorte a partir del
    /// histograma completo, que se pide al soltar el deslizador.
    pub fn histograma_rapido(&self, lado_maximo: u32) -> Histograma {
        let dim = self.dim();
        let factor = (dim.ancho.max(dim.alto) as f32 / lado_maximo as f32).max(1.0);
        let reducida = Dimensiones::nueva(
            ((dim.ancho as f32 / factor) as u32).max(1),
            ((dim.alto as f32 / factor) as u32).max(1),
        )
        .expect("la reducción no puede dar cero: está forzada a 1 como mínimo");
        let region = Region { x: 0.0, y: 0.0, ancho: dim.ancho as f32, alto: dim.alto as f32 };
        pf_image::histograma::calcular(&self.vista(region, reducida, Revelado::Actual))
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use pf_image::CANALES;

    /// Escribe un PNG de prueba y devuelve su ruta.
    fn foto_de_prueba(nombre: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("pf_prueba_documento");
        std::fs::create_dir_all(&dir).unwrap();
        let ruta = dir.join(nombre);
        let mut buf = image::RgbImage::new(16, 16);
        for (x, y, p) in buf.enumerate_pixels_mut() {
            let v = ((x + y) * 8).min(255) as u8;
            *p = image::Rgb([v, v, v]);
        }
        buf.save(&ruta).unwrap();
        ruta
    }

    fn doc(nombre: &str) -> Documento {
        Documento::abrir(&foto_de_prueba(nombre)).unwrap()
    }

    #[test]
    fn el_original_sobrevive_a_cualquier_cantidad_de_operaciones() {
        // Esta es LA prueba del no destructivo. Si falla, la promesa entera
        // del programa es mentira.
        let mut d = doc("intacto.png");
        let copia: Vec<f32> = d.original().datos().to_vec();

        d.ajustar(Operacion::Exposicion { pasos: 2.0 });
        d.ajustar(Operacion::Contraste { cantidad: 80.0 });
        let _ = d.completa(Revelado::Actual);
        let _ = d.vista(
            Region { x: 0.0, y: 0.0, ancho: 16.0, alto: 16.0 },
            Dimensiones::nueva(8, 8).unwrap(),
            Revelado::Actual,
        );
        d.ajustar(Operacion::Exposicion { pasos: -3.0 });

        assert_eq!(d.original().datos(), copia.as_slice());
    }

    #[test]
    fn mover_el_mismo_deslizador_no_encadena_operaciones() {
        let mut d = doc("una_sola.png");
        for i in 1..=30 {
            d.ajustar(Operacion::Exposicion { pasos: i as f32 / 10.0 });
        }
        assert_eq!(d.operaciones().len(), 1);
        assert_eq!(d.operaciones()[0], Operacion::Exposicion { pasos: 3.0 });
    }

    #[test]
    fn devolver_el_deslizador_a_cero_deja_el_documento_como_estaba() {
        let mut d = doc("vuelta_a_cero.png");
        d.ajustar(Operacion::Exposicion { pasos: 1.5 });
        d.ajustar(Operacion::Exposicion { pasos: 0.0 });
        assert!(d.operaciones().is_empty());
        assert!(!d.hay_cambios());
    }

    #[test]
    fn el_resultado_no_depende_de_que_deslizador_se_toco_primero() {
        // El motivo de que exista `Operacion::orden`.
        let mut a = doc("orden_a.png");
        a.ajustar(Operacion::Exposicion { pasos: 1.0 });
        a.ajustar(Operacion::Contraste { cantidad: 40.0 });

        let mut b = doc("orden_b.png");
        b.ajustar(Operacion::Contraste { cantidad: 40.0 });
        b.ajustar(Operacion::Exposicion { pasos: 1.0 });

        assert_eq!(a.completa(Revelado::Actual).datos(), b.completa(Revelado::Actual).datos());
    }

    #[test]
    fn deshacer_y_rehacer_recorren_el_historial_en_los_dos_sentidos() {
        let mut d = doc("historial.png");
        d.ajustar(Operacion::Exposicion { pasos: 1.0 });
        d.ajustar(Operacion::Contraste { cantidad: 20.0 });
        assert_eq!(d.operaciones().len(), 2);

        assert!(d.deshacer());
        assert_eq!(d.operaciones().len(), 1);
        assert!(d.deshacer());
        assert!(d.operaciones().is_empty());
        assert!(!d.deshacer(), "no debería poder deshacer más allá del principio");

        assert!(d.rehacer());
        assert_eq!(d.operaciones().len(), 1);
        assert!(d.rehacer());
        assert_eq!(d.operaciones().len(), 2);
        assert!(!d.rehacer());
    }

    #[test]
    fn hacer_algo_nuevo_despues_de_deshacer_borra_el_futuro() {
        let mut d = doc("futuro.png");
        d.ajustar(Operacion::Exposicion { pasos: 1.0 });
        d.ajustar(Operacion::Contraste { cantidad: 20.0 });
        d.deshacer();
        assert!(d.puede_rehacer());

        d.ajustar(Operacion::Contraste { cantidad: -50.0 });
        assert!(!d.puede_rehacer(), "el futuro que no ocurrió no debería quedar disponible");
    }

    #[test]
    fn el_antes_y_el_despues_se_muestrean_igual_para_que_la_comparacion_sea_honrada() {
        let mut d = doc("comparar.png");
        let region = Region { x: 0.0, y: 0.0, ancho: 16.0, alto: 16.0 };
        let destino = Dimensiones::nueva(5, 5).unwrap();

        let antes = d.vista(region, destino, Revelado::ComoSeAbrio);
        d.ajustar(Operacion::Exposicion { pasos: 0.0 }); // neutra: no cambia nada
        let despues = d.vista(region, destino, Revelado::Actual);

        assert_eq!(antes.datos(), despues.datos());
        assert_eq!(antes.dim(), destino);
    }

    #[test]
    fn el_histograma_rapido_tiene_la_misma_forma_que_el_completo() {
        // No se comparan los recuentos uno a uno —son muestras distintas—,
        // sino dónde cae el centro de masas. Si el reducido estuviera
        // calculado sobre otra cosa, el centro se iría.
        let mut d = doc("histogramas.png");
        d.ajustar(Operacion::Exposicion { pasos: 0.5 });

        let centro = |h: &Histograma| -> f64 {
            let total: u64 = h.luminancia.iter().map(|x| *x as u64).sum();
            let suma: u64 = h.luminancia.iter().enumerate().map(|(i, c)| i as u64 * *c as u64).sum();
            suma as f64 / total.max(1) as f64
        };

        let completo = d.histograma();
        let rapido = d.histograma_rapido(8);
        assert!(
            (centro(&completo) - centro(&rapido)).abs() < 12.0,
            "completo {} vs rápido {}",
            centro(&completo),
            centro(&rapido)
        );
    }

    #[test]
    fn exportar_lo_revelado_da_un_archivo_distinto_del_original() {
        let d = {
            let mut d = doc("exportado.png");
            d.ajustar(Operacion::Exposicion { pasos: 1.0 });
            d
        };
        let revelada = d.completa(Revelado::Actual);
        let sin_revelar = d.completa(Revelado::ComoSeAbrio);
        assert_ne!(revelada.datos(), sin_revelar.datos());
        assert_eq!(revelada.datos().len(), d.dim().pixeles() * CANALES);
    }
}
