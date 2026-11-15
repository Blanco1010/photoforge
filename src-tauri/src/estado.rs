//! Lo que la aplicación recuerda entre una llamada y la siguiente.

use parking_lot::Mutex;
use pf_core::{Error, Resultado};
use pf_document::Documento;

/// Por qué un solo documento y no una lista de pestañas: PhotoForge todavía no
/// tiene pestañas, y un `HashMap<IdDocumento, Documento>` con sus identificadores
/// que hay que generar, pasar por la frontera y validar sería infraestructura
/// para una función que no existe. El día que haya pestañas, este tipo cambia
/// y los comandos casi no se enteran, porque todos pasan por los dos métodos
/// de abajo.
#[derive(Default)]
pub struct Estado {
    documento: Mutex<Option<Documento>>,
}

impl Estado {
    /// Hace algo con el documento abierto, o devuelve el error de que no hay
    /// ninguno.
    ///
    /// Por qué un closure y no un `fn documento() -> Option<&Documento>`: el
    /// documento vive detrás de un mutex y no se puede devolver una
    /// referencia a su interior sin que el guard siga vivo. Con el closure, el
    /// bloqueo dura exactamente lo que dura la operación y no hay forma de
    /// olvidarse de soltarlo.
    pub fn con<T>(&self, f: impl FnOnce(&Documento) -> T) -> Resultado<T> {
        let guard = self.documento.lock();
        match guard.as_ref() {
            Some(d) => Ok(f(d)),
            None => Err(Error::SinDocumento),
        }
    }

    pub fn con_mut<T>(&self, f: impl FnOnce(&mut Documento) -> T) -> Resultado<T> {
        let mut guard = self.documento.lock();
        match guard.as_mut() {
            Some(d) => Ok(f(d)),
            None => Err(Error::SinDocumento),
        }
    }

    pub fn poner(&self, documento: Documento) {
        *self.documento.lock() = Some(documento);
    }

    /// Cierra el documento y libera sus píxeles.
    ///
    /// Esto importa más de lo que parece: una foto de 50 Mpx son 600 MB. Sin
    /// un cierre explícito, abrir cinco fotos seguidas dejaría la memoria
    /// alta hasta que se abra la sexta, y el usuario vería un programa que
    /// «se come la RAM».
    pub fn cerrar(&self) {
        *self.documento.lock() = None;
    }

    pub fn hay_documento(&self) -> bool {
        self.documento.lock().is_some()
    }
}
