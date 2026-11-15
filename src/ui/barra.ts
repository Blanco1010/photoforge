/**
 * La barra de herramientas.
 */

import type { Ficha } from "../lib/api";
import { el } from "../lib/dom";

export interface AccionesBarra {
  abrir: () => void;
  deshacer: () => void;
  rehacer: () => void;
  restablecer: () => void;
  exportar: () => void;
  ajustarAVentana: () => void;
  unoAUno: () => void;
  /** Mantener pulsado enseña el original; soltar vuelve al revelado. */
  comparar: (activo: boolean) => void;
}

export class Barra {
  readonly nodo: HTMLElement;
  private readonly titulo: HTMLSpanElement;
  private readonly subtitulo: HTMLSpanElement;
  private readonly botones: Record<string, HTMLButtonElement> = {};

  constructor(acciones: AccionesBarra) {
    this.titulo = el("span", { clase: "barra__titulo", texto: "PhotoForge" });
    this.subtitulo = el("span", {
      clase: "barra__subtitulo",
      texto: "no hay ninguna foto abierta",
    });

    const boton = (
      id: string,
      texto: string,
      titulo: string,
      alClic: () => void,
      clase = "",
    ): HTMLButtonElement => {
      const b = el("button", { clase: `boton ${clase}`.trim(), texto, titulo, alClic });
      this.botones[id] = b;
      return b;
    };

    // El botón de comparar es el único que funciona manteniéndolo pulsado, y
    // es a propósito: comparar es un vistazo de medio segundo, no un modo en
    // el que entrar y del que acordarse de salir. Así es como lo hacen todos
    // los reveladores, y por eso no lleva estado que enseñar.
    const comparar = el("button", {
      clase: "boton boton--comparar",
      texto: "Antes",
      titulo: "Mantén pulsado para ver el original (o la tecla \\)",
    });
    const pulsar = () => {
      comparar.classList.add("boton--activo");
      acciones.comparar(true);
    };
    const soltar = () => {
      comparar.classList.remove("boton--activo");
      acciones.comparar(false);
    };
    comparar.addEventListener("pointerdown", pulsar);
    comparar.addEventListener("pointerup", soltar);
    comparar.addEventListener("pointerleave", soltar);
    comparar.addEventListener("pointercancel", soltar);
    this.botones.comparar = comparar;

    this.nodo = el(
      "header",
      { clase: "barra" },
      el("div", { clase: "barra__identidad" }, this.titulo, this.subtitulo),
      el(
        "div",
        { clase: "barra__grupo" },
        boton("abrir", "Abrir", "Abrir una foto (Ctrl+O)", acciones.abrir, "boton--principal"),
      ),
      el(
        "div",
        { clase: "barra__grupo" },
        boton("deshacer", "Deshacer", "Deshacer (Ctrl+Z)", acciones.deshacer),
        boton("rehacer", "Rehacer", "Rehacer (Ctrl+Y)", acciones.rehacer),
        boton("restablecer", "Restablecer", "Quitar todos los ajustes", acciones.restablecer),
      ),
      el(
        "div",
        { clase: "barra__grupo" },
        comparar,
        boton("ajustar", "Ajustar", "Ver la foto entera (tecla 0)", acciones.ajustarAVentana),
        boton("unoAUno", "1:1", "Un píxel por píxel (tecla 1)", acciones.unoAUno),
      ),
      el(
        "div",
        { clase: "barra__grupo barra__grupo--final" },
        boton("exportar", "Exportar", "Guardar una copia revelada (Ctrl+E)", acciones.exportar),
      ),
    );
  }

  sincronizar(ficha: Ficha | null): void {
    if (ficha) {
      this.titulo.textContent = ficha.nombre;
      this.subtitulo.textContent = `${ficha.ancho} × ${ficha.alto}${
        ficha.hay_cambios ? " · con ajustes" : ""
      }`;
    } else {
      this.titulo.textContent = "PhotoForge";
      this.subtitulo.textContent = "no hay ninguna foto abierta";
    }

    const hay = ficha !== null;
    this.botones.deshacer.disabled = !ficha?.puede_deshacer;
    this.botones.rehacer.disabled = !ficha?.puede_rehacer;
    this.botones.restablecer.disabled = !ficha?.hay_cambios;
    this.botones.comparar.disabled = !ficha?.hay_cambios;
    this.botones.exportar.disabled = !hay;
    this.botones.ajustar.disabled = !hay;
    this.botones.unoAUno.disabled = !hay;
  }
}
