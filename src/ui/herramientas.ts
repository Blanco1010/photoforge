/**
 * La columna de herramientas.
 *
 * # Tres iconos, y por qué no hay más
 *
 * Solo está lo que existe. No hay pincel, ni selección, ni clonar, ni capas:
 * poner sus iconos apagados «para enseñar el plan» sería llenar la pantalla
 * de puertas que no llevan a ningún sitio.
 *
 * Tampoco hay lupa, y esa sí hubo que decidirla. La rueda ya amplía anclada
 * al cursor y el doble clic ya ajusta a la ventana; con `Ver` y sus teclas
 * `0` y `1`, un icono de lupa sería la CUARTA puerta al mismo acto, y la peor
 * de las cuatro. Quitarla no retira ninguna capacidad: se niega a inventar
 * una segunda forma de hacer lo que ya se hace mejor.
 *
 * Y no hay cuadro de «Opciones de herramienta» debajo, como en GIMP, porque
 * mano, cuentagotas y comparar no tienen ni una opción: sería una caja vacía.
 *
 * # La condición para que esta columna pueda existir
 *
 * Hasta ahora, arrastrar con el izquierdo desplazaba SIEMPRE. En cuanto hay
 * modos excluyentes, con el cuentagotas elegido ese arrastre deja de
 * desplazar, y si nada lo explica el usuario arrastra, la foto no se mueve y
 * la herramienta parece rota.
 *
 * Por eso **el arrastre con el botón central desplaza en cualquier
 * herramienta**, está escrito en el tooltip del cuentagotas —que es el
 * momento exacto en que hace falta saberlo— y la vuelta a la mano es una
 * tecla. Eso no es un extra: sin esa salida, esta columna sería peor que la
 * barra de botones que sustituye.
 */

import type { Ficha } from "../lib/api";
import { el, icono } from "../lib/dom";

export type Herramienta = "mano" | "cuentagotas";

/** La mano abierta de Krita y Photoshop. GIMP no trae una en su caja. */
const RUTA_MANO =
  "M8 13V6.5a1.5 1.5 0 0 1 3 0V12m0-1.5a1.5 1.5 0 0 1 3 0V12m0-1a1.5 1.5 0 0 1 " +
  "3 0v4.5A5.5 5.5 0 0 1 12.5 21H11a5 5 0 0 1-5-5v-1.2a1.5 1.5 0 0 1 2.6-1L9.5 15";

/** La pipeta diagonal, que esta sí es la de GIMP. */
const RUTA_CUENTAGOTAS = "M15.5 4.5l4 4 M17.5 6.5 9 15l-1.5 4.5L3 21l1.5-4.5L13 8z";

/** Un cuadro partido por la mitad. GIMP no tiene nada equivalente. */
const RUTA_COMPARAR = "M4 5h16v14H4z M12 5v14";
const RELLENO_COMPARAR = "M4.5 5.5h7v13h-7z";

export class Herramientas {
  readonly nodo: HTMLElement;

  private activa: Herramienta = "mano";
  private readonly botones = new Map<Herramienta, HTMLButtonElement>();
  private readonly comparar: HTMLButtonElement;
  /** Los tres botones en orden, para el tabindex rodante. */
  private readonly orden: HTMLButtonElement[] = [];

  alCambiar?: (h: Herramienta) => void;
  alComparar?: (activo: boolean) => void;

  constructor() {
    const mano = this.modo(
      "mano",
      RUTA_MANO,
      "Mano — arrastrar para desplazar (M)",
    );
    const cuentagotas = this.modo(
      "cuentagotas",
      RUTA_CUENTAGOTAS,
      "Cuentagotas — clic para leer el color en pantalla; el botón central sigue desplazando (C)",
    );

    this.comparar = el("button", {
      clase: "herramienta herramienta--momentanea",
      type: "button",
      tabindex: "-1",
      titulo: "Mantén pulsado para ver el original (\\)",
      "aria-label": "Ver el original",
    });
    this.comparar.append(icono(RUTA_COMPARAR, RELLENO_COMPARAR));

    // El pulsar-y-mantener, traído tal cual de la barra que sustituye: es un
    // vistazo de medio segundo, no un modo en el que entrar y del que
    // acordarse de salir.
    const pulsar = () => this.ponerComparar(true);
    const soltar = () => this.ponerComparar(false);
    this.comparar.addEventListener("pointerdown", pulsar);
    this.comparar.addEventListener("pointerup", soltar);
    this.comparar.addEventListener("pointerleave", soltar);
    this.comparar.addEventListener("pointercancel", soltar);
    // Y con el teclado, que hace falta de verdad: un botón que recibe el foco
    // y no hace nada al pulsar Enter es literalmente un control muerto.
    this.comparar.addEventListener("keydown", (e) => {
      if ((e.key === "Enter" || e.key === " ") && !e.repeat) {
        e.preventDefault();
        pulsar();
      }
    });
    this.comparar.addEventListener("keyup", (e) => {
      if (e.key === "Enter" || e.key === " ") soltar();
    });

    this.orden.push(mano, cuentagotas, this.comparar);

    this.nodo = el(
      "div",
      { clase: "herramientas", role: "toolbar", "aria-orientation": "vertical", "aria-label": "Herramientas" },
      mano,
      cuentagotas,
      // La raya no es decoración: sin ella, un botón que se mantiene pulsado
      // parece un tercer modo, y el usuario intentaría hacerle clic y soltar
      // esperando quedarse dentro.
      el("div", { clase: "herramientas__separador", role: "separator" }),
      this.comparar,
    );

    this.nodo.addEventListener("keydown", (e) => this.alPulsar(e));
    this.marcar();
  }

  private modo(cual: Herramienta, ruta: string, ayuda: string): HTMLButtonElement {
    const b = el("button", {
      clase: "herramienta",
      type: "button",
      tabindex: "-1",
      titulo: ayuda,
      "aria-label": ayuda,
      "aria-pressed": "false",
      alClic: () => this.elegir(cual),
    });
    b.append(icono(ruta));
    this.botones.set(cual, b);
    return b;
  }

  elegir(cual: Herramienta): void {
    if (this.activa === cual) return;
    this.activa = cual;
    this.marcar();
    this.alCambiar?.(cual);
  }

  get herramienta(): Herramienta {
    return this.activa;
  }

  private ponerComparar(activo: boolean): void {
    this.comparar.classList.toggle("herramienta--pulsada", activo);
    this.alComparar?.(activo);
  }

  private marcar(): void {
    for (const [cual, boton] of this.botones) {
      const puesta = cual === this.activa;
      boton.classList.toggle("herramienta--activa", puesta);
      boton.setAttribute("aria-pressed", String(puesta));
    }
    // Tabindex rodante: la columna entera es UNA parada de tabulador, para
    // que llegar con Tab al panel no obligue a pasar por tres botones.
    for (const b of this.orden) b.tabIndex = b === this.botones.get(this.activa) ? 0 : -1;
  }

  /** Apaga la columna entera cuando no hay foto, igual que el panel. */
  sincronizar(ficha: Ficha | null): void {
    for (const b of this.botones.values()) b.disabled = ficha === null;
    this.comparar.disabled = !ficha?.hay_cambios;
    if (!ficha?.hay_cambios) this.ponerComparar(false);
  }

  /**
   * Las teclas de las herramientas.
   *
   * Viven aquí y no en la tabla de acciones de `main.ts` porque no son
   * acciones: son modos, no tienen entrada de menú, y meterlas en una
   * estructura cuyo otro consumidor es precisamente el menú obligaría a
   * inventar una bandera de «entrada que no se pinta».
   *
   * Devuelve `true` si la tecla era suya.
   */
  atajo(e: KeyboardEvent): boolean {
    if (e.ctrlKey || e.metaKey || e.altKey) return false;
    switch (e.key.toLowerCase()) {
      case "m":
        this.elegir("mano");
        return true;
      case "c":
        this.elegir("cuentagotas");
        return true;
      case "\\":
        if (!e.repeat) this.ponerComparar(true);
        return true;
      default:
        return false;
    }
  }

  /**
   * El fin del gesto de comparar.
   *
   * Esto NO lleva ninguna guarda, a propósito: si el `keyup` se filtrara por
   * algún motivo, el lienzo se quedaría enseñando el original para siempre
   * porque el gesto no tendría quien lo terminara.
   */
  atajoSoltar(e: KeyboardEvent): boolean {
    if (e.key !== "\\") return false;
    this.ponerComparar(false);
    return true;
  }

  private alPulsar(e: KeyboardEvent): void {
    if (e.key !== "ArrowUp" && e.key !== "ArrowDown") return;
    const i = this.orden.indexOf(e.target as HTMLButtonElement);
    if (i < 0) return;
    const paso = e.key === "ArrowDown" ? 1 : -1;
    // En círculo y SIN activar: activan Enter y Espacio, que en un <button>
    // llegan como `click` de serie.
    this.orden[(i + paso + this.orden.length) % this.orden.length].focus();
    e.preventDefault();
    e.stopPropagation();
  }
}
