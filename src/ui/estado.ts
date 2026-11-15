/**
 * La barra de estado.
 *
 * # Por qué esto tiene archivo propio para seis trozos de texto
 *
 * Porque su única responsabilidad es una garantía, y la garantía es que
 * **cada dato tiene su casilla y cada casilla un solo escritor**.
 *
 * Antes el pie era un nodo que se vaciaba y se volvía a escribir entero, y lo
 * escribía todo el mundo. El resultado era un fallo real: cada repintado del
 * lienzo escribía ahí el zoom, así que «Exportado: C:\…\foto-revelada.jpg»
 * —o un error de exportación en rojo— duraba hasta que el ratón rozaba la
 * foto o se redimensionaba la ventana. El mensaje más importante de la sesión
 * se perdía por mover la rueda.
 *
 * Con seis nodos y seis métodos que no se pisan, eso deja de ser algo que
 * haya que recordar y pasa a ser imposible.
 *
 * # Por qué va dentro de la columna central
 *
 * No cruza la ventana: ocupa solo el ancho del lienzo, entre la columna de
 * herramientas y el panel, como en GIMP. Es lo que permite que el panel suba
 * hasta debajo de los menús, y es lo que más cambia la silueta de la
 * aplicación por menos CSS.
 */

import type { Ficha } from "../lib/api";
import { el } from "../lib/dom";
import type { Herramienta } from "./herramientas";
import type { InfoVista } from "./lienzo";

/** Con coma decimal, que es la de aquí. */
function cifra(n: number, decimales: number): string {
  return n.toFixed(decimales).replace(".", ",");
}

export class BarraEstado {
  readonly nodo: HTMLElement;

  private readonly original: HTMLElement;
  private readonly mensaje: HTMLElement;
  private readonly pantalla: HTMLElement;
  private readonly muestra: HTMLElement;
  private readonly cifras: HTMLElement;

  /*
   * La casilla del cuentagotas depende de DOS cosas —que haya foto y que la
   * herramienta sea el cuentagotas—, y cada una la cambia un método distinto.
   * Guardarlas y recalcular en un solo sitio es lo que impide que la casilla
   * se quede enseñando «pincha para leer un color» sobre una ventana vacía,
   * que es justo lo que pasaba.
   */
  private hayFoto = false;
  private conCuentagotas = false;
  private readonly zoom: HTMLElement;
  private readonly tamano: HTMLElement;
  private readonly ms: HTMLElement;

  constructor() {
    this.original = el("span", { clase: "estado__original", texto: "ORIGINAL" });
    this.mensaje = el("span", { clase: "estado__mensaje", texto: "Listo" });
    // Un cuadradito con el color de verdad y los números al lado.
    //
    // La primera versión eran solo los números, y Juan dijo que el
    // cuentagotas «no funciona»: funcionaba, pero escribía texto gris de 11
    // píxeles al fondo de la pantalla, lejos de donde estaba mirando. Una
    // lectura que no se nota es una lectura que no existe, y el color se ve
    // con el rabillo del ojo mientras los números hay que ir a leerlos.
    this.muestra = el("span", { clase: "estado__muestra" });
    this.cifras = el("span", { clase: "estado__cifras" });
    this.pantalla = el(
      "span",
      { clase: "estado__pantalla" },
      // El rótulo se queda: es donde vive el asterisco. Lo que lee el
      // cuentagotas es el color EN PANTALLA, no la luz de la escena, y eso
      // tiene que estar en lo que el usuario ve siempre y no en un tooltip.
      el("span", { clase: "estado__rotulo", texto: "En pantalla" }),
      this.muestra,
      this.cifras,
    );
    this.zoom = el("span", { clase: "estado__zoom" });
    this.tamano = el("span", { clase: "estado__tamano" });
    this.ms = el("span", { clase: "estado__ms" });

    this.original.hidden = true;
    this.pantalla.hidden = true;

    this.nodo = el(
      "footer",
      { clase: "estado" },
      this.original,
      this.mensaje,
      this.pantalla,
      this.zoom,
      this.tamano,
      this.ms,
    );
  }

  /** La casilla 2. El único sitio de la aplicación que escribe mensajes. */
  decir(texto: string, esError = false): void {
    this.mensaje.textContent = texto;
    this.mensaje.classList.toggle("estado__mensaje--error", esError);
    this.mensaje.title = texto;
  }

  /** La casilla 1: se ve solo mientras se compara, y en color de acento. */
  comparando(activo: boolean): void {
    this.original.hidden = !activo;
  }

  /**
   * La casilla 3, que solo existe con el cuentagotas elegido.
   *
   * Se llama «En pantalla» y no «Color» a propósito: lo que lee el
   * cuentagotas es el valor sRGB de 8 bits del lienzo ya pintado, no la luz
   * lineal de la escena. El asterisco va en el rótulo que el usuario lee
   * siempre, no en un tooltip ni en un comentario del fuente.
   */
  ponerColor(rgb: [number, number, number] | null): void {
    if (!rgb) {
      this.muestra.style.background = "transparent";
      this.muestra.classList.remove("estado__muestra--con-color");
      this.cifras.textContent = "pincha para leer un color";
      this.pantalla.classList.remove("estado__pantalla--con-color");
      return;
    }
    const [r, g, b] = rgb;
    const hex = [r, g, b].map((v) => v.toString(16).padStart(2, "0")).join("");
    this.muestra.style.background = `rgb(${r} ${g} ${b})`;
    this.muestra.classList.add("estado__muestra--con-color");
    // El hexadecimal además de los tres números: es lo que la gente copia y
    // pega, y ocupa menos que repetir «En pantalla».
    this.cifras.textContent = `${r} ${g} ${b}  #${hex}`;
    this.pantalla.classList.add("estado__pantalla--con-color");
    this.pantalla.title =
      "El color tal como se ve en pantalla (sRGB de 8 bits), no la luz lineal de la escena";
  }

  /** Enseña u oculta la casilla del cuentagotas según la herramienta. */
  ponerHerramienta(h: Herramienta): void {
    this.conCuentagotas = h === "cuentagotas";
    this.ponerColor(null);
    this.refrescarPantalla();
  }

  private refrescarPantalla(): void {
    this.pantalla.hidden = !(this.hayFoto && this.conCuentagotas);
  }

  /** Las casillas 4 y 6, que escribe el lienzo en cada cuadro. */
  ponerVista(info: InfoVista | null): void {
    if (!info) {
      this.zoom.textContent = "";
      this.ms.textContent = "";
      return;
    }
    this.zoom.textContent = `${cifra(info.zoom * 100, info.zoom < 1 ? 1 : 0)} %`;
    this.ms.textContent = `${info.ms.toFixed(0)} ms`;
  }

  /** La casilla 5, que solo cambia al abrir o cerrar una foto. */
  ponerFoto(ficha: Ficha | null): void {
    this.hayFoto = ficha !== null;
    this.refrescarPantalla();
    if (!ficha) {
      this.tamano.textContent = "";
      this.ponerVista(null);
      this.ponerColor(null);
      this.comparando(false);
      return;
    }
    this.tamano.textContent =
      `${ficha.ancho} × ${ficha.alto} · ${cifra(ficha.megapixeles, 1)} Mpx`;
  }
}
