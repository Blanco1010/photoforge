/**
 * El panel de la derecha: histograma, ajustes y metadatos.
 */

import { api, type Ficha, type Histograma, type Operacion } from "../lib/api";
import { el, vaciar } from "../lib/dom";

/** Dibuja el histograma en un lienzo pequeño. */
class DibujoHistograma {
  readonly nodo: HTMLCanvasElement;
  private ultimo: Histograma | null = null;

  constructor() {
    this.nodo = el("canvas", { clase: "histograma__lienzo" });
    new ResizeObserver(() => this.repintar()).observe(this.nodo);
  }

  poner(h: Histograma | null): void {
    this.ultimo = h;
    this.repintar();
  }

  private repintar(): void {
    const ctx = this.nodo.getContext("2d");
    if (!ctx) return;
    const dpr = window.devicePixelRatio || 1;
    const ancho = this.nodo.clientWidth;
    const alto = this.nodo.clientHeight;
    if (ancho <= 0 || alto <= 0) return;
    this.nodo.width = Math.round(ancho * dpr);
    this.nodo.height = Math.round(alto * dpr);
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, ancho, alto);

    const h = this.ultimo;
    if (!h) return;

    // Los tres canales se dibujan sumándose con «lighter»: donde coinciden
    // los tres sale blanco, que es exactamente lo que significa —ahí la foto
    // es gris— y es como lo enseña cualquier editor. Dibujarlos opacos uno
    // encima de otro escondería el que quedara debajo.
    const series: [number[], string][] = [
      [h.rojo, "rgba(255, 70, 70, 0.55)"],
      [h.verde, "rgba(70, 230, 120, 0.55)"],
      [h.azul, "rgba(80, 140, 255, 0.55)"],
    ];
    ctx.globalCompositeOperation = "lighter";
    for (const [serie, color] of series) {
      this.area(ctx, serie, h.maximo, ancho, alto, color);
    }

    ctx.globalCompositeOperation = "source-over";
    ctx.strokeStyle = "rgba(235, 240, 250, 0.75)";
    ctx.lineWidth = 1;
    this.linea(ctx, h.luminancia, h.maximo, ancho, alto);
  }

  private camino(
    ctx: CanvasRenderingContext2D,
    serie: number[],
    maximo: number,
    ancho: number,
    alto: number,
  ): void {
    ctx.beginPath();
    for (let i = 0; i < serie.length; i++) {
      const x = (i / (serie.length - 1)) * ancho;
      // Se recorta al máximo en vez de escalar por el valor real: el máximo ya
      // viene calculado ignorando los extremos (ver `histograma.rs`), así que
      // las columnas quemadas se salen por arriba a propósito.
      const y = alto - Math.min(1, serie[i] / maximo) * alto;
      if (i === 0) ctx.moveTo(x, y);
      else ctx.lineTo(x, y);
    }
  }

  private area(
    ctx: CanvasRenderingContext2D,
    serie: number[],
    maximo: number,
    ancho: number,
    alto: number,
    color: string,
  ): void {
    this.camino(ctx, serie, maximo, ancho, alto);
    ctx.lineTo(ancho, alto);
    ctx.lineTo(0, alto);
    ctx.closePath();
    ctx.fillStyle = color;
    ctx.fill();
  }

  private linea(
    ctx: CanvasRenderingContext2D,
    serie: number[],
    maximo: number,
    ancho: number,
    alto: number,
  ): void {
    this.camino(ctx, serie, maximo, ancho, alto);
    ctx.stroke();
  }
}

/** Un deslizador con su número y su vuelta a cero. */
class Deslizador {
  readonly nodo: HTMLDivElement;
  private readonly rango: HTMLInputElement;
  private readonly lectura: HTMLSpanElement;

  constructor(
    etiqueta: string,
    private readonly min: number,
    private readonly max: number,
    paso: number,
    private readonly formato: (v: number) => string,
    private readonly alMover: (v: number) => void,
    private readonly alSoltar: () => void,
  ) {
    this.rango = el("input", {
      clase: "deslizador__rango",
      type: "range",
      min: String(min),
      max: String(max),
      step: String(paso),
      value: "0",
    });
    this.lectura = el("span", { clase: "deslizador__lectura", texto: formato(0) });

    this.rango.addEventListener("input", () => {
      const v = Number(this.rango.value);
      this.lectura.textContent = this.formato(v);
      this.alMover(v);
    });
    // `change` llega al soltar el ratón; es el momento de pedir lo caro (el
    // histograma completo) sin hacerlo sesenta veces durante el arrastre.
    this.rango.addEventListener("change", () => this.alSoltar());

    const reiniciar = el("button", {
      clase: "deslizador__cero",
      texto: "0",
      titulo: "Volver a cero",
      alClic: () => this.poner(0, true),
    });

    this.nodo = el(
      "div",
      { clase: "deslizador" },
      el(
        "div",
        { clase: "deslizador__cabecera" },
        el("span", { clase: "deslizador__etiqueta", texto: etiqueta }),
        this.lectura,
        reiniciar,
      ),
      this.rango,
    );
  }

  /** Coloca el deslizador. Con `avisar` a `false` no dispara el callback. */
  poner(valor: number, avisar: boolean): void {
    const v = Math.min(this.max, Math.max(this.min, valor));
    this.rango.value = String(v);
    this.lectura.textContent = this.formato(v);
    if (avisar) {
      this.alMover(v);
      this.alSoltar();
    }
  }
}

export class Panel {
  readonly nodo: HTMLElement;
  private readonly histograma = new DibujoHistograma();
  private readonly avisoRecorte: HTMLParagraphElement;
  private readonly exposicion: Deslizador;
  private readonly contraste: Deslizador;
  private readonly resumen: HTMLDListElement;
  private readonly listaExif: HTMLDivElement;
  private readonly detalleExif: HTMLDetailsElement;

  constructor(
    private readonly alAjustar: (op: Operacion, definitivo: boolean) => void,
  ) {
    this.avisoRecorte = el("p", { clase: "histograma__aviso" });

    this.exposicion = new Deslizador(
      "Exposición",
      -5,
      5,
      0.05,
      (v) => `${v > 0 ? "+" : ""}${v.toFixed(2)} EV`,
      (v) => this.alAjustar({ clase: "exposicion", pasos: v }, false),
      () => this.alAjustar({ clase: "exposicion", pasos: Number(this.valorExposicion()) }, true),
    );
    this.contraste = new Deslizador(
      "Contraste",
      -100,
      100,
      1,
      (v) => `${v > 0 ? "+" : ""}${v.toFixed(0)}`,
      (v) => this.alAjustar({ clase: "contraste", cantidad: v }, false),
      () => this.alAjustar({ clase: "contraste", cantidad: Number(this.valorContraste()) }, true),
    );

    this.resumen = el("dl", { clase: "metadatos__resumen" });
    this.listaExif = el("div", { clase: "metadatos__lista" });
    this.detalleExif = el(
      "details",
      { clase: "metadatos__detalle" },
      el("summary", { texto: "Todos los campos EXIF" }),
      this.listaExif,
    );

    this.nodo = el(
      "aside",
      { clase: "panel" },
      el(
        "section",
        { clase: "panel__seccion" },
        el("h2", { clase: "panel__titulo", texto: "Histograma" }),
        el("div", { clase: "histograma" }, this.histograma.nodo),
        this.avisoRecorte,
      ),
      el(
        "section",
        { clase: "panel__seccion" },
        el("h2", { clase: "panel__titulo", texto: "Revelado" }),
        this.exposicion.nodo,
        this.contraste.nodo,
      ),
      el(
        "section",
        { clase: "panel__seccion panel__seccion--crece" },
        el("h2", { clase: "panel__titulo", texto: "Metadatos" }),
        this.resumen,
        this.detalleExif,
      ),
    );
  }

  private valorExposicion(): string {
    return (
      this.exposicion.nodo.querySelector<HTMLInputElement>(".deslizador__rango")?.value ?? "0"
    );
  }

  private valorContraste(): string {
    return this.contraste.nodo.querySelector<HTMLInputElement>(".deslizador__rango")?.value ?? "0";
  }

  /**
   * Pone los deslizadores donde diga la ficha, sin disparar cambios.
   *
   * Hace falta porque deshacer y rehacer cambian el documento desde fuera de
   * los deslizadores: si no se recolocaran, el panel enseñaría +2 EV sobre
   * una foto que ya no los tiene.
   */
  sincronizar(ficha: Ficha | null): void {
    const exp = ficha?.operaciones.find((o) => o.clase === "exposicion");
    const con = ficha?.operaciones.find((o) => o.clase === "contraste");
    this.exposicion.poner(exp && exp.clase === "exposicion" ? exp.pasos : 0, false);
    this.contraste.poner(con && con.clase === "contraste" ? con.cantidad : 0, false);
    this.ponerMetadatos(ficha);
  }

  ponerHistograma(h: Histograma | null, completo: boolean): void {
    this.histograma.poner(h);
    if (!h || !completo) {
      if (!h) this.avisoRecorte.textContent = "";
      return;
    }
    const porcentaje = (n: number) => ((n / Math.max(1, h.muestras)) * 100).toFixed(1);
    const partes: string[] = [];
    if (h.recortados_negros > 0) partes.push(`${porcentaje(h.recortados_negros)} % en negro`);
    if (h.recortados_blancos > 0) partes.push(`${porcentaje(h.recortados_blancos)} % quemado`);
    this.avisoRecorte.textContent = partes.length ? partes.join(" · ") : "Sin recorte";
    this.avisoRecorte.className = partes.length
      ? "histograma__aviso histograma__aviso--atencion"
      : "histograma__aviso";
  }

  private ponerMetadatos(ficha: Ficha | null): void {
    vaciar(this.resumen);
    vaciar(this.listaExif);
    if (!ficha) {
      this.detalleExif.hidden = true;
      return;
    }

    const m = ficha.metadatos;
    const filas: [string, string | null][] = [
      ["Tamaño", `${ficha.ancho} × ${ficha.alto} · ${ficha.megapixeles.toFixed(1)} Mpx`],
      ["En memoria", `${ficha.megabytes_en_memoria} MB en RGB lineal`],
      ["Cámara", m.camara],
      ["Objetivo", m.objetivo],
      ["Focal", m.distancia_focal],
      ["Diafragma", m.diafragma],
      ["Obturación", m.obturacion],
      ["ISO", m.iso],
      ["Fecha", m.fecha],
    ];
    for (const [clave, valor] of filas) {
      if (!valor) continue;
      this.resumen.append(
        el("dt", { texto: clave }),
        el("dd", { texto: valor.replace(/^"|"$/g, "") }),
      );
    }

    if (m.completo.length === 0) {
      this.detalleExif.hidden = true;
      this.resumen.append(
        el("dt", { texto: "EXIF" }),
        el("dd", { clase: "metadatos__vacio", texto: "el archivo no trae ninguno" }),
      );
      return;
    }
    this.detalleExif.hidden = false;
    for (const campo of m.completo) {
      this.listaExif.append(
        el(
          "div",
          { clase: "metadatos__campo" },
          el("span", { clase: "metadatos__clave", texto: campo.etiqueta }),
          el("span", { clase: "metadatos__valor", texto: campo.valor }),
        ),
      );
    }
  }

  /** Apaga los ajustes cuando no hay nada abierto. */
  habilitar(activo: boolean): void {
    for (const r of this.nodo.querySelectorAll<HTMLInputElement | HTMLButtonElement>(
      ".deslizador__rango, .deslizador__cero",
    )) {
      r.disabled = !activo;
    }
  }
}

/** Pide el histograma y lo pinta, sin dejar que se solapen dos peticiones. */
export class PeticionHistograma {
  private enVuelo = false;
  private pendiente: boolean | null = null;

  constructor(private readonly panel: Panel) {}

  async pedir(completo: boolean): Promise<void> {
    if (this.enVuelo) {
      // Si ya hay una en marcha, se queda apuntada la más exigente de las
      // dos: un «completo» pendiente no se degrada a «rápido».
      this.pendiente = this.pendiente === true ? true : completo;
      return;
    }
    this.enVuelo = true;
    try {
      let siguiente: boolean | null = completo;
      while (siguiente !== null) {
        const esta: boolean = siguiente;
        this.pendiente = null;
        const h = await api.histograma(esta);
        this.panel.ponerHistograma(h, esta);
        siguiente = this.pendiente;
      }
    } catch {
      // Sin documento abierto. No hay histograma que enseñar y ya está.
      this.panel.ponerHistograma(null, false);
    } finally {
      this.enVuelo = false;
      this.pendiente = null;
    }
  }
}
