/**
 * La frontera con Rust.
 *
 * Todo lo que cruza el puente pasa por aquí, y no hay ni un `invoke` suelto en
 * el resto de la interfaz. El motivo no es el orden: es que los nombres de los
 * comandos y la forma de sus argumentos son un contrato con el otro lado, y un
 * contrato repartido por doce archivos no se puede cambiar sin romper algo.
 */

import { invoke } from "@tauri-apps/api/core";

export type Operacion =
  | { clase: "exposicion"; pasos: number }
  | { clase: "contraste"; cantidad: number }
  | { clase: "curva"; activa: boolean };

/** De dónde vienen los píxeles: un RAW sin interpretar o un archivo ya revelado. */
export type Procedencia = "revelada" | "escena";

export interface CampoExif {
  etiqueta: string;
  valor: string;
}

export interface Metadatos {
  camara: string | null;
  objetivo: string | null;
  distancia_focal: string | null;
  diafragma: string | null;
  obturacion: string | null;
  iso: string | null;
  fecha: string | null;
  completo: CampoExif[];
}

export interface Ficha {
  ruta: string;
  nombre: string;
  ancho: number;
  alto: number;
  megapixeles: number;
  megabytes_en_memoria: number;
  procedencia: Procedencia;
  metadatos: Metadatos;
  operaciones: Operacion[];
  puede_deshacer: boolean;
  puede_rehacer: boolean;
  hay_cambios: boolean;
}

export interface Histograma {
  rojo: number[];
  verde: number[];
  azul: number[];
  luminancia: number[];
  maximo: number;
  recortados_negros: number;
  recortados_blancos: number;
  muestras: number;
}

export type Formato =
  | { tipo: "jpeg"; calidad: number }
  | { tipo: "png" }
  | { tipo: "tiff16" };

export interface Peticion {
  x: number;
  y: number;
  ancho_region: number;
  alto_region: number;
  ancho: number;
  alto: number;
  con_operaciones: boolean;
}

export const api = {
  abrir: (ruta: string) => invoke<Ficha>("abrir", { ruta }),
  cerrar: () => invoke<void>("cerrar"),
  ficha: () => invoke<Ficha>("ficha"),
  ajustar: (operacion: Operacion) => invoke<Ficha>("ajustar", { operacion }),
  restablecer: () => invoke<Ficha>("restablecer"),
  deshacer: () => invoke<Ficha>("deshacer"),
  rehacer: () => invoke<Ficha>("rehacer"),
  histograma: (completo: boolean) => invoke<Histograma>("histograma", { completo }),
  exportar: (ruta: string, formato: Formato) => invoke<string>("exportar", { ruta, formato }),
  extensiones: () => invoke<string[]>("extensiones_soportadas"),

  /**
   * Pide los píxeles de un cuadro.
   *
   * Devuelve un `ArrayBuffer` con RGBA en crudo y NO un JSON: el comando de
   * Rust responde con `tauri::ipc::Response`, que se salta la serialización.
   * El porqué está en `src-tauri/src/comandos.rs`; el resumen es que 5,7 MB de
   * bytes se convertirían en más de 20 MB de texto treinta veces por segundo.
   */
  renderizar: (peticion: Peticion) => invoke<ArrayBuffer>("renderizar", { peticion }),
};
