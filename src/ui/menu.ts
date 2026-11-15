/**
 * La barra de menús.
 *
 * # El invariante del que sale todo lo demás
 *
 * **Hay un menú abierto si y solo si el foco está dentro de la barra.**
 *
 * Las catorce reglas de comportamiento de abajo no son catorce decisiones
 * sueltas: son catorce maneras de mantener esa frase. Y de ella salen gratis
 * dos cosas que si no habría que programar a mano:
 *
 * - Los atajos de una sola tecla de `main.ts` no se disparan con un menú
 *   abierto, porque la barra se queda con el teclado mientras lo tiene.
 * - `main.ts` no necesita saber que el menú existe, ni consultar ninguna
 *   bandera `menuAbierto` cada vez que se añada un atajo nuevo.
 *
 * El estado completo de esta clase cabe en un campo: `abierto`.
 *
 * # Por qué una tabla de acciones y no botones sueltos
 *
 * El mismo dato —«Exportar como…, Ctrl+E, llama a exportar, se activa si hay
 * foto»— lo necesitan tres sitios: el menú lo pinta, el teclado lo dispara y
 * el rótulo del atajo sale de él. Antes estaba escrito tres veces, y ya había
 * un desajuste real: la barra vieja rotulaba «Ctrl+Y» mientras el teclado
 * atendía además Ctrl+Mayús+Z. Con la tabla, lo que se enseña y lo que
 * dispara salen del mismo sitio y no se pueden separar.
 */

import type { Ficha } from "../lib/api";
import { el } from "../lib/dom";

/**
 * Un atajo declarado en CAMPOS y no en una cadena como `"Ctrl+E"`.
 *
 * Una cadena habría que interpretarla, y un mini-lenguaje con su analizador es
 * exactamente la clase de infraestructura que este proyecto evita. Con campos,
 * `rotulo` y `coincide` son cuatro líneas cada uno y no pueden discrepar.
 */
export interface Atajo {
  ctrl?: boolean;
  mayus?: boolean;
  /** En minúscula, como la devuelve `e.key.toLowerCase()`. */
  tecla: string;
}

export interface Accion {
  id: string;
  /** Una función cuando el texto cambia según el estado, como «Ocultar/Enseñar el panel». */
  texto: string | (() => string);
  /** El que se ENSEÑA a la derecha de la entrada. */
  atajo?: Atajo;
  /** Otro que también dispara y no se enseña, como Ctrl+Mayús+Z para rehacer. */
  alias?: Atajo;
  separadorAntes?: boolean;
  activa: (f: Ficha | null) => boolean;
  hacer: () => void;
}

export interface GrupoMenu {
  titulo: string;
  entradas: Accion[];
}

/** `{ctrl: true, tecla: "e"}` -> `"Ctrl+E"`. */
export function rotulo(a: Atajo): string {
  const partes: string[] = [];
  if (a.ctrl) partes.push("Ctrl");
  if (a.mayus) partes.push("Mayús");
  partes.push(a.tecla.length === 1 ? a.tecla.toUpperCase() : a.tecla);
  return partes.join("+");
}

/**
 * ¿Este evento de teclado es este atajo?
 *
 * Compara los tres bits y nada más. Un atajo que no esté en la tabla ni se
 * enseña ni funciona: no hay forma de que la interfaz mienta sobre las teclas.
 */
export function coincide(e: KeyboardEvent, a?: Atajo): boolean {
  if (!a) return false;
  const ctrl = e.ctrlKey || e.metaKey;
  return (
    ctrl === !!a.ctrl && e.shiftKey === !!a.mayus && e.key.toLowerCase() === a.tecla
  );
}

export class Menu {
  readonly nodo: HTMLElement;

  private readonly titulos: HTMLButtonElement[] = [];
  private readonly listas = new Map<HTMLButtonElement, HTMLElement>();
  private readonly acciones = new Map<HTMLButtonElement, Accion[]>();

  /** El título cuyo menú está desplegado. Este es todo el estado que hay. */
  private abierto: HTMLButtonElement | null = null;

  /**
   * El escucha de «clic fuera» se suscribe al abrir y se retira al cerrar.
   *
   * Dejarlo puesto para siempre lo ejecutaría en cada clic de toda la sesión
   * para no hacer nada. Atar la suscripción al estado es lo que impide que se
   * queden desincronizados.
   */
  private readonly clicFuera = (e: PointerEvent) => {
    if (!this.nodo.contains(e.target as Node)) this.cerrar(false);
  };

  constructor(
    grupos: GrupoMenu[],
    private readonly fichaActual: () => Ficha | null,
  ) {
    this.nodo = el("div", { clase: "barramenus", role: "menubar" });

    for (const grupo of grupos) {
      const titulo = el("button", {
        clase: "menu__titulo",
        type: "button",
        texto: grupo.titulo,
        role: "menuitem",
        "aria-haspopup": "true",
        "aria-expanded": "false",
        tabindex: this.titulos.length === 0 ? "0" : "-1",
      });
      const lista = el("div", { clase: "menu__lista", role: "menu" });
      const entradas: Accion[] = [];

      for (const accion of grupo.entradas) {
        if (accion.separadorAntes) {
          lista.append(el("div", { clase: "menu__separador", role: "separator" }));
        }
        const entrada = el(
          "button",
          {
            clase: "menu__entrada",
            type: "button",
            role: "menuitem",
            tabindex: "-1",
            // Enter y Espacio sobre un <button> llegan como `click` de serie,
            // así que la activación vive SOLO aquí. Si además se manejaran en
            // el keydown, Enter exportaría dos veces.
            alClic: () => {
              // Cerrar ANTES de ejecutar: estas acciones levantan diálogos
              // nativos o tardan, y un menú abierto sobre un diálogo del
              // sistema es el artefacto clásico de un menú hecho a mano.
              this.cerrar(false);
              accion.hacer();
            },
          },
          el("span", { clase: "menu__texto" }),
          el("span", {
            clase: "menu__atajo",
            texto: accion.atajo ? rotulo(accion.atajo) : "",
          }),
        );
        lista.append(entrada);
        entradas.push(accion);
      }

      titulo.addEventListener("click", () => {
        if (this.abierto === titulo) this.cerrar();
        else this.abrirMenu(titulo);
      });
      // Pasar el ratón cambia de menú SOLO si ya había uno abierto. Una barra
      // que se despliega al pasar por encima tiende emboscadas: el camino del
      // lienzo al panel pasa justo por debajo.
      titulo.addEventListener("pointerenter", () => {
        if (this.abierto && this.abierto !== titulo) this.abrirMenu(titulo);
      });

      this.titulos.push(titulo);
      this.listas.set(titulo, lista);
      this.acciones.set(titulo, entradas);
      this.nodo.append(el("div", { clase: "menu" }, titulo, lista));
    }

    // Un solo keydown en la raíz: títulos y desplegables están dentro, así que
    // el burbujeo los ve todos. Esto solo funciona porque cada lista se
    // construye DENTRO del `.menu` de su título; si algún día una se pintara
    // en otro sitio del DOM, esta regla se rompería en silencio.
    this.nodo.addEventListener("keydown", (e) => this.alPulsar(e));

    // Cerrar al perder el foco, con `relatedTarget`: con un `blur` por entrada
    // se cerraría al pasar de una entrada a la de al lado.
    this.nodo.addEventListener("focusout", (e) => {
      const va = (e as FocusEvent).relatedTarget as Node | null;
      if (!va || !this.nodo.contains(va)) this.cerrar(false);
    });
    window.addEventListener("blur", () => this.cerrar(false));
  }

  private entradasVivas(titulo: HTMLButtonElement): HTMLButtonElement[] {
    const lista = this.listas.get(titulo);
    if (!lista) return [];
    return [...lista.querySelectorAll<HTMLButtonElement>(".menu__entrada:not([disabled])")];
  }

  private abrirMenu(titulo: HTMLButtonElement): void {
    if (this.abierto && this.abierto !== titulo) this.cerrarSolo(this.abierto);

    // El activado se calcula AL ABRIR y nunca al construir. Un menú que solo
    // se ve desplegado no puede estar desfasado si decide en ese instante, y
    // eso quita la mitad del trabajo de `sincronizar()`.
    //
    // Se toca SOLO `disabled` y el texto: jamás se reconstruye el DOM de la
    // lista. Vaciarla destruiría el nodo que va a recibir el foco, y el
    // invariante se rompería en el mismo acto de abrir.
    const ficha = this.fichaActual();
    const lista = this.listas.get(titulo)!;
    const acciones = this.acciones.get(titulo)!;
    const nodos = [...lista.querySelectorAll<HTMLButtonElement>(".menu__entrada")];
    nodos.forEach((nodo, i) => {
      const accion = acciones[i];
      nodo.disabled = !accion.activa(ficha);
      const texto = typeof accion.texto === "function" ? accion.texto() : accion.texto;
      nodo.querySelector(".menu__texto")!.textContent = texto;
    });

    titulo.setAttribute("aria-expanded", "true");
    this.abierto = titulo;
    this.enfocarTitulo(titulo, false);
    document.addEventListener("pointerdown", this.clicFuera, true);

    // El `?? titulo` cubre sin caso especial un menú con todas las entradas
    // apagadas: el foco se queda en la barra y el invariante aguanta.
    (this.entradasVivas(titulo)[0] ?? titulo).focus();
  }

  private cerrarSolo(titulo: HTMLButtonElement): void {
    titulo.setAttribute("aria-expanded", "false");
  }

  /**
   * Cierra el menú abierto.
   *
   * `devolverFoco` es `false` cuando el motivo del cierre es que el usuario se
   * iba a otro sitio: un clic en el lienzo o un Tab. Devolverle el foco al
   * título entonces sería quitárselo al sitio al que quería ir.
   */
  private cerrar(devolverFoco = true): void {
    if (!this.abierto) return;
    const titulo = this.abierto;
    this.cerrarSolo(titulo);
    this.abierto = null;
    document.removeEventListener("pointerdown", this.clicFuera, true);
    if (devolverFoco) titulo.focus();
  }

  /** Mueve el tabindex rodante. La barra entera es UNA parada de tabulador. */
  private enfocarTitulo(nuevo: HTMLButtonElement, mover = true): void {
    for (const t of this.titulos) t.tabIndex = t === nuevo ? 0 : -1;
    if (mover) nuevo.focus();
  }

  private vecino(titulo: HTMLButtonElement, paso: number): HTMLButtonElement {
    const i = this.titulos.indexOf(titulo);
    const n = this.titulos.length;
    return this.titulos[(i + paso + n) % n];
  }

  private alPulsar(e: KeyboardEvent): void {
    const objetivo = e.target as HTMLElement;

    if (this.abierto) {
      const titulo = this.abierto;

      // Tab y los aceleradores con Ctrl se dejan pasar. Tab porque el usuario
      // ha dicho que quiere irse a otra parte y secuestrarlo es pelearse con
      // él; los Ctrl porque en cualquier programa nativo un acelerador
      // funciona con el menú abierto — y el menú se cierra porque la acción
      // va a cambiar el documento por debajo.
      if (e.key === "Tab" || e.ctrlKey || e.metaKey) {
        this.cerrar(false);
        return;
      }

      const vivas = this.entradasVivas(titulo);
      const actual = vivas.indexOf(objetivo as HTMLButtonElement);

      switch (e.key) {
        case "Escape":
          // Escape significa «me he equivocado al abrir esto», no «sácame de
          // aquí»: quien lo pulsa quiere probar el menú de al lado.
          this.cerrar();
          break;
        case "ArrowDown":
          vivas[(actual + 1 + vivas.length) % Math.max(1, vivas.length)]?.focus();
          break;
        case "ArrowUp":
          vivas[(actual - 1 + vivas.length) % Math.max(1, vivas.length)]?.focus();
          break;
        case "Home":
          vivas[0]?.focus();
          break;
        case "End":
          vivas[vivas.length - 1]?.focus();
          break;
        case "ArrowRight":
          this.abrirMenu(this.vecino(titulo, 1));
          break;
        case "ArrowLeft":
          this.abrirMenu(this.vecino(titulo, -1));
          break;
      }
      // Todo lo que llegue aquí se queda en la barra, atendido o no: es lo
      // que hace que los atajos de una sola tecla de `main.ts` no se
      // disparen mientras hay un menú desplegado.
      e.preventDefault();
      e.stopPropagation();
      return;
    }

    // Menú cerrado: solo se atiende si el foco está en un título.
    if (!(objetivo instanceof HTMLButtonElement) || !this.titulos.includes(objetivo)) return;

    switch (e.key) {
      case "ArrowRight":
        this.enfocarTitulo(this.vecino(objetivo, 1));
        break;
      case "ArrowLeft":
        this.enfocarTitulo(this.vecino(objetivo, -1));
        break;
      case "Home":
        this.enfocarTitulo(this.titulos[0]);
        break;
      case "End":
        this.enfocarTitulo(this.titulos[this.titulos.length - 1]);
        break;
      case "ArrowDown":
        this.abrirMenu(objetivo);
        break;
      default:
        return;
    }
    // Sin esto, la flecha abajo desplaza la página por debajo del menú.
    e.preventDefault();
    e.stopPropagation();
  }
}
