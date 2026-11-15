/**
 * El panel de la derecha: histograma, ajustes y metadatos.
 */

import { api, type ClaseCantidad, type Ficha, type Histograma, type Operacion } from "../lib/api";
import { el, vaciar, type Hijos } from "../lib/dom";

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
  /**
   * El tooltip va en la ETIQUETA y no en el deslizador entero.
   *
   * Si estuviera en el contenedor saltaría también al arrastrar, y un globo
   * de texto tapando la foto justo mientras la estás juzgando es lo contrario
   * de una ayuda.
   */
  private readonly ayuda?: string;

  constructor(
    etiqueta: string,
    private readonly min: number,
    private readonly max: number,
    paso: number,
    private readonly formato: (v: number) => string,
    private readonly alMover: (v: number) => void,
    private readonly alSoltar: () => void,
    ayuda?: string,
    /**
     * A dónde vuelve el botón ↺.
     *
     * Casi siempre es cero, pero el del degradado vuelve a su valor de
     * partida, que es el que dejó el arrastre. El rótulo del botón dice
     * «Volver al valor de partida», así que tiene que ser verdad.
     */
    private readonly partida = 0,
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
    this.ayuda = ayuda;

    this.rango.addEventListener("input", () => {
      const v = Number(this.rango.value);
      this.lectura.textContent = this.formato(v);
      this.alMover(v);
    });
    // `change` llega al soltar el ratón; es el momento de pedir lo caro (el
    // histograma completo) sin hacerlo sesenta veces durante el arrastre.
    this.rango.addEventListener("change", () => this.alSoltar());

    // El glifo es ↺ y no «0», y el motivo es el mismo que el del cuentagotas:
    // no estaba roto, decía mal lo que hacía. «0» es además la tecla de
    // «ajustar a la ventana», y con ocho deslizadores en pantalla la interfaz
    // enseñaría ocho veces el símbolo de una tecla que hace otra cosa.
    const reiniciar = el("button", {
      clase: "deslizador__cero",
      texto: "↺",
      titulo: "Volver al valor de partida",
      alClic: () => this.poner(this.partida, true),
    });

    this.nodo = el(
      "div",
      { clase: "deslizador" },
      el(
        "div",
        { clase: "deslizador__cabecera" },
        el("span", { clase: "deslizador__etiqueta", texto: etiqueta, titulo: this.ayuda ?? "" }),
        this.lectura,
        reiniciar,
      ),
      this.rango,
    );
  }

  /** Lo que marca ahora mismo. */
  valor(): number {
    return Number(this.rango.value);
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

/**
 * Una sección plegable del panel.
 *
 * # Por qué una función de seis líneas y no una clase `Seccion`
 *
 * La versión con clase tendría `plegar()`, `estaPlegada` y un evento propio:
 * estado que mantener para algo que el navegador ya hace solo. `<details>` da
 * el plegado gratis, es accesible con el teclado de serie, y **ya se usa en
 * este mismo archivo** para «Todos los campos EXIF». Así no entra ni un
 * concepto nuevo en el proyecto: se usa dos veces el mismo.
 *
 * El `<h2>` se queda DENTRO del `<summary>`, que la especificación permite: el
 * panel es el único sitio de la aplicación con encabezados, y perderlos
 * convertiría el lateral en una sopa de divs para un lector de pantalla.
 *
 * # Por qué se recuerda si estaba plegada
 *
 * Por el mismo motivo que existe `recuperar()` en `main.ts`: en desarrollo,
 * cualquier cambio en un `.ts` recarga la página, y sin esto el panel se
 * desplegaría entero cada vez.
 */
function seccion(
  id: string,
  titulo: string,
  abiertaPorOmision: boolean,
  ...hijos: Hijos
): HTMLDetailsElement {
  return seccionAvisada(id, titulo, abiertaPorOmision, undefined, ...hijos);
}

function seccionAvisada(
  id: string,
  titulo: string,
  abiertaPorOmision: boolean,
  alPlegar: ((abierta: boolean) => void) | undefined,
  ...hijos: Hijos
): HTMLDetailsElement {
  const clave = `photoforge.panel.${id}`;
  const guardado = localStorage.getItem(clave);
  const abierta = guardado === null ? abiertaPorOmision : guardado === "abierto";

  const nodo = el(
    "details",
    { clase: "panel__seccion", open: abierta },
    el(
      "summary",
      { clase: "panel__resumen" },
      el("h2", { clase: "panel__titulo", texto: titulo }),
    ),
    ...hijos,
  );
  nodo.addEventListener("toggle", () => {
    localStorage.setItem(clave, nodo.open ? "abierto" : "cerrado");
    alPlegar?.(nodo.open);
  });
  return nodo;
}

/** Una casilla con su etiqueta. Para lo que está o no está. */
class Casilla {
  readonly nodo: HTMLLabelElement;
  private readonly entrada: HTMLInputElement;

  constructor(etiqueta: string, ayuda: string, private readonly alCambiar: (v: boolean) => void) {
    this.entrada = el("input", { clase: "casilla__control", type: "checkbox" });
    this.entrada.addEventListener("change", () => this.alCambiar(this.entrada.checked));
    this.nodo = el(
      "label",
      { clase: "casilla", titulo: ayuda },
      this.entrada,
      el("span", { clase: "casilla__etiqueta", texto: etiqueta }),
    );
  }

  poner(valor: boolean): void {
    this.entrada.checked = valor;
  }
}

export class Panel {
  readonly nodo: HTMLElement;
  private readonly histograma = new DibujoHistograma();
  private readonly avisoRecorte: HTMLParagraphElement;
  private readonly exposicion: Deslizador;
  private readonly curva: Casilla;
  /**
   * Los cinco deslizadores de -100 a +100, por su clase.
   *
   * Un mapa y no cinco campos: `sincronizar` recorre esto en una línea en vez
   * de hacer cinco `find` con cinco comparaciones de tipo, y añadir el
   * siguiente control es añadir una entrada.
   */
  private readonly cantidades = new Map<ClaseCantidad, Deslizador>();
  private readonly luzDegradado: Deslizador;
  private readonly sinDegradado: HTMLParagraphElement;
  private readonly conDegradado: HTMLDivElement;
  /** Avisa al lienzo de si hay que dibujar las líneas del degradado. */
  alVerDegradado?: (visible: boolean) => void;
  private readonly resumen: HTMLDListElement;
  private readonly listaExif: HTMLDivElement;
  private readonly detalleExif: HTMLDetailsElement;

  constructor(
    private readonly alAjustar: (op: Operacion, definitivo: boolean) => void,
    /** Para que el panel pueda devolver la ficha que le dé un comando suyo. */
    private readonly alRefrescar?: (f: Ficha) => void,
  ) {
    this.avisoRecorte = el("p", { clase: "histograma__aviso" });

    this.exposicion = new Deslizador(
      "Exposición",
      // ±6 y no ±5: la exposición automática de un RAW puede llegar a +6
      // pasos, y con el tope en 5 el deslizador enseñaría 5,00 sobre una foto
      // que lleva 5,12. Un control que miente sobre lo que hay es peor que no
      // tenerlo.
      -6,
      6,
      0.05,
      (v) => `${v > 0 ? "+" : ""}${v.toFixed(2).replace(".", ",")} EV`,
      (v) => this.alAjustar({ clase: "exposicion", pasos: v }, false),
      () => this.alAjustar({ clase: "exposicion", pasos: this.exposicion.valor() }, true),
      "Cuánta luz. Cada paso es el doble, igual que en la cámara.",
    );
    // Los cinco de -100 a +100 salen de la misma función.
    //
    // Sin ella, cada uno sería un bloque de siete líneas en el que la cadena
    // de la clase aparece DOS veces —al mover y al soltar— más una tercera al
    // sincronizar. Quince apariciones de cinco cadenas en tres sitios es
    // exactamente el error que la tabla de acciones del menú existe para
    // matar.
    const cantidad = (etiqueta: string, clase: ClaseCantidad, ayuda: string) => {
      const d = new Deslizador(
        etiqueta,
        -100,
        100,
        1,
        (v) => `${v > 0 ? "+" : ""}${v.toFixed(0)}`,
        (v) => this.alAjustar({ clase, cantidad: v }, false),
        () => this.alAjustar({ clase, cantidad: d.valor() }, true),
        ayuda,
      );
      this.cantidades.set(clase, d);
      return d.nodo;
    };

    // La curva de luces: una casilla y no un deslizador. El porqué está en
    // `Operacion::CurvaDeTono`, en el lado de Rust.
    this.curva = new Casilla(
      "Doblar las luces",
      "En vez de cortar lo que se pasa de blanco, lo dobla para que no se queme. Los medios tonos no se tocan.",
      (v) => this.alAjustar({ clase: "curva", activa: v }, true),
    );

    this.luzDegradado = new Deslizador(
      "Luz del degradado",
      -6,
      6,
      0.05,
      (v) => `${v > 0 ? "+" : ""}${v.toFixed(2).replace(".", ",")} EV`,
      (v) => void api.ajustarDegradado(v, false).then((f) => this.alRefrescar?.(f)),
      () => void api.ajustarDegradado(this.luzDegradado.valor(), true).then((f) => this.alRefrescar?.(f)),
      "Quita o pone luz dentro de la franja, como el cristal degradado que se pone delante del objetivo. Se aplica antes de la curva de luces, así que oscurecer un cielo quemado puede devolverle detalle. La franja es RECTA y no sabe lo que hay debajo: lo que asome dentro de ella —las cumbres, las copas de los árboles, un tejado— se oscurece igual que el cielo.",
      -0.75,
    );
    this.sinDegradado = el("p", {
      clase: "metadatos__vacio",
      texto:
        "Ningún degradado puesto. Elige la herramienta Degradado (D) y arrastra sobre la foto: desde donde quieras el efecto entero hasta donde se acabe.",
    });
    this.conDegradado = el("div", {}, this.luzDegradado.nodo);
    this.conDegradado.hidden = true;

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
      // El orden no es arbitrario. El histograma es el instrumento que se
      // MIRA mientras la otra mano arrastra el deslizador, así que va pegado
      // a los deslizadores y por encima de ellos.
      seccion(
        "histograma",
        "Histograma",
        true,
        el("div", { clase: "histograma" }, this.histograma.nodo),
        this.avisoRecorte,
      ),
      // Los grupos son la misma raya que dibuja `Operacion::orden()` en Rust:
      // leído de arriba abajo, el panel ENSEÑA el orden en que se aplican las
      // operaciones, que es la decisión más difícil de explicar del programa.
      // Y de paso es el orden de Lightroom, así que los dedos de quien venga
      // de ahí aciertan.
      seccion(
        "luz",
        "Luz",
        true,
        this.exposicion.nodo,
        cantidad("Luces", "luces",
          "Aclara o recupera lo más brillante sin mover el blanco de la foto. Actúa por brillo y no por zonas, así que dos cosas igual de claras en esquinas distintas se mueven igual. Sobre un cielo que llegó quemado no hay detalle que recuperar: bajarlo solo lo deja gris."),
        cantidad("Sombras", "sombras",
          "Abre o cierra lo más oscuro sin mover el negro de la foto. Actúa por brillo y no por zonas: no se puede abrir la sombra del árbol sin tocar la chaqueta oscura, y es eso lo que garantiza que no aparezcan halos."),
        cantidad("Blancos", "blancos",
          "Decide dónde cae el blanco. Es el único junto con Negros que puede empujar un tono fuera del rango: al soltarlo, el aviso del histograma te dice cuánto se ha quemado."),
        cantidad("Negros", "negros",
          "Decide dónde cae el negro. Es el único junto con Blancos que puede empujar un tono fuera del rango: al soltarlo, el aviso del histograma te dice cuánto ha quedado en negro."),
      ),
      // «Interpretación» y no «Tono»: en este proyecto «tono» ya significa el
      // matiz de un color y además el valor tonal, y una tercera acepción
      // sobraría. Es la palabra que usa el propio `orden()`: el contraste es
      // una decisión de interpretación.
      seccion(
        "interpretacion",
        "Interpretación",
        true,
        this.curva.nodo,
        cantidad("Contraste", "contraste",
          "Separa o junta los tonos alrededor del medio. Se aplica sobre el valor que verías en pantalla y no sobre la luz, para que el deslizador se sienta parejo de punta a punta."),
      ),
      // El degradado va al final de los ajustes y no entre medias: no es una
      // décima entrada de la lista ordenada, es un ajuste que modula la
      // exposición del revelado entero.
      seccionAvisada(
        "degradado",
        "Degradado",
        true,
        (abierta) => this.alVerDegradado?.(abierta),
        this.sinDegradado,
        this.conDegradado,
      ),
      // Metadatos empieza CERRADA: se lee una vez al abrir la foto y nunca
      // más, es la que crece, y así enseña el plegado en el primer arranque
      // sin que nadie tenga que contarlo.
      seccion("metadatos", "Metadatos", false, this.resumen, this.detalleExif),
    );
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
    this.exposicion.poner(exp?.clase === "exposicion" ? exp.pasos : 0, false);

    for (const [clase, deslizador] of this.cantidades) {
      const op = ficha?.operaciones.find((o) => o.clase === clase);
      deslizador.poner(op && "cantidad" in op ? op.cantidad : 0, false);
    }

    const cur = ficha?.operaciones.find((o) => o.clase === "curva");
    this.curva.poner(cur?.clase === "curva" && cur.activa);

    const hay = ficha?.degradado != null;
    this.sinDegradado.hidden = hay;
    this.conDegradado.hidden = !hay;
    this.luzDegradado.poner(ficha?.degradado?.pasos ?? -0.75, false);
    this.ponerMetadatos(ficha);
  }

  ponerHistograma(h: Histograma | null, completo: boolean): void {
    this.histograma.poner(h);
    if (!h || !completo) {
      if (!h) this.avisoRecorte.textContent = "";
      return;
    }
    const porcentaje = (n: number) =>
      ((n / Math.max(1, h.muestras)) * 100).toFixed(1).replace(".", ",");
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
      // Es la única lectura en pantalla que distingue un giro tuyo de uno que
      // ya traía la cámara: la orientación del archivo se consume al abrir y
      // no deja rastro aquí.
      ["Girada", ficha.giro],
      [
        "Origen",
        ficha.procedencia === "escena"
          ? "RAW de cámara (luz medida, sin interpretar)"
          : "archivo ya revelado",
      ],
      ["Tamaño", `${ficha.ancho} × ${ficha.alto} · ${ficha.megapixeles.toFixed(1).replace(".", ",")} Mpx`],
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

  /** Despliega la sección del degradado. La llama la herramienta. */
  abrirDegradado(): void {
    const s = this.nodo.querySelectorAll<HTMLDetailsElement>(".panel__seccion")[3];
    if (s && !s.open) s.open = true;
    this.alVerDegradado?.(s?.open ?? false);
  }

  /** Si la sección del degradado está desplegada. */
  get degradadoALaVista(): boolean {
    return this.nodo.querySelectorAll<HTMLDetailsElement>(".panel__seccion")[3]?.open ?? false;
  }

  /** Apaga los ajustes cuando no hay nada abierto. */
  habilitar(activo: boolean): void {
    for (const r of this.nodo.querySelectorAll<HTMLInputElement | HTMLButtonElement>(
      ".deslizador__rango, .deslizador__cero, .casilla__control",
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
