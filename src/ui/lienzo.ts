/**
 * El lienzo: zoom, desplazamiento y pintado.
 *
 * # La idea que hace que esto vaya rápido
 *
 * El lienzo NO tiene la foto. Tiene un zoom, un punto central y el tamaño de
 * la ventana; con esas tres cosas calcula qué rectángulo de la foto se ve y a
 * qué resolución hace falta, y se lo pide a Rust ya revelado. Lo que vuelve
 * mide exactamente lo que se va a pintar, ni un píxel más.
 *
 * Por eso mover el deslizador de exposición cuesta lo mismo con una foto de
 * 2 Mpx que con una de 50: el trabajo depende del tamaño de la ventana.
 */

import { api, type Degradado, type Ficha, type Punto } from "../lib/api";
import { el, porCuadro } from "../lib/dom";
import type { Herramienta } from "./herramientas";

export interface InfoVista {
  zoom: number;
  ancho: number;
  alto: number;
  ms: number;
}

/** Arrastre mínimo, en píxeles de pantalla, para crear un degradado. */
const ARRASTRE_MINIMO = 12;
/** A qué distancia de un tirador se considera que lo has agarrado. */
const RADIO_DE_ACIERTO = 10;

/** Hasta dónde se puede ampliar. 32× ya enseña el píxel como un cuadrado. */
const ZOOM_MAXIMO = 32;
/** Y hasta dónde alejar: por debajo de esto no se distingue nada. */
const ZOOM_MINIMO = 0.02;

export class Lienzo {
  readonly nodo: HTMLDivElement;
  private readonly canvas: HTMLCanvasElement;
  private readonly ctx: CanvasRenderingContext2D;
  /**
   * El lienzo de los adornos: las líneas y los tiradores del degradado.
   *
   * # Por qué un segundo lienzo y no pintar encima del primero
   *
   * Tres motivos, y el primero es un fallo concreto: el cuentagotas lee
   * `getImageData` del lienzo pintado, así que una raya dibujada ahí se
   * leería como si fuera el color de la foto al pinchar encima.
   *
   * El segundo: el bucle de `unCuadro` protege que borrar y pintar ocurran
   * sin un `await` en medio, y mover una línea obligaría a guardar y reponer
   * los 5,7 MB del último cuadro, además de hacer que el tirador heredara la
   * latencia del puente con Rust.
   *
   * El tercero: con el adorno inerte al ratón (`pointer-events: none`),
   * TODOS los eventos siguen llegando al lienzo de la foto, así que la
   * captura del puntero, el botón central que desplaza siempre, la rueda
   * anclada y el doble clic no se tocan ni una línea.
   */
  private readonly adorno: HTMLCanvasElement;
  private readonly ctxAdorno: CanvasRenderingContext2D;

  private ficha: Ficha | null = null;
  /** Píxeles de pantalla (CSS) por píxel de imagen. */
  private zoom = 1;
  /** Punto de la imagen que queda en el centro de la vista. */
  private cx = 0;
  private cy = 0;
  /** Mientras está a `true` se pinta el original, para comparar. */
  private comparando = false;
  /** Qué hace el botón izquierdo. El central siempre desplaza. */
  private herramienta: Herramienta = "mano";
  /** El degradado puesto, en coordenadas de la foto como se ve. */
  private degradado: Degradado | null = null;
  /** Mientras es `false` no se dibuja el adorno. Lo manda el panel. */
  private verAdorno = false;
  /** El gesto que se está haciendo con el ratón, si hay alguno. */
  private gesto: {
    cual: "crear" | "desde" | "hasta" | "medio";
    desde: Punto;
    hasta: Punto;
    /** Dónde se pulsó, para el mínimo de arrastre y para el tirador central. */
    origen: Punto;
    nacido: boolean;
  } | null = null;

  /**
   * Solo puede haber un render en vuelo.
   *
   * Sin esto, arrastrar el ratón encola cincuenta peticiones y la imagen
   * llega siempre tarde, porque Rust va contestando a las viejas. Con esto,
   * mientras una está en marcha las demás se resumen en «hay que repetir», y
   * al terminar se lanza UNA con el estado más reciente.
   */
  private pintando = false;
  private repetir = false;

  /** Para que la barra de estado pueda enseñar el zoom y el tiempo. */
  alPintar?: (info: InfoVista) => void;
  /** El color leído por el cuentagotas, o `null` si se pinchó fuera de la foto. */
  alMuestrear?: (rgb: [number, number, number] | null) => void;
  /** El degradado que el usuario está dibujando o moviendo. */
  alDegradar?: (desde: Punto, hasta: Punto, definitivo: boolean) => void;

  constructor() {
    this.canvas = el("canvas", { clase: "lienzo__canvas" });
    this.adorno = el("canvas", { clase: "lienzo__adorno" });
    this.nodo = el("div", { clase: "lienzo" }, this.canvas, this.adorno);
    const ctx = this.canvas.getContext("2d", { alpha: true });
    if (!ctx) throw new Error("este WebView no da un contexto 2D");
    this.ctx = ctx;
    const ctxAdorno = this.adorno.getContext("2d");
    if (!ctxAdorno) throw new Error("este WebView no da un contexto 2D");
    this.ctxAdorno = ctxAdorno;

    this.conectarRaton();
    new ResizeObserver(porCuadro(() => this.refrescar())).observe(this.nodo);
  }

  cargar(ficha: Ficha): void {
    this.ficha = ficha;
    this.ajustarAVentana();
  }

  /**
   * Cambia la foto por su versión nueva sin perder la vista.
   *
   * La regla es una y sirve para el giro, para deshacer y para la vuelta
   * atrás: **la vista no se mueve nunca salvo que la foto haya cambiado de
   * forma.** Así, girar 90° reencuadra —porque si no, la foto se saldría de
   * la ventana— y un espejo o un giro de 180° conservan el zoom y el sitio
   * exactos, que es lo que uno espera si estaba mirando un detalle.
   *
   * Esto arregla además un fallo que ya estaba: `cargar` era el único sitio
   * que renovaba la ficha guardada, así que deshacer dejaba al lienzo con
   * una ficha vieja. Hoy era invisible porque ninguna operación cambiaba las
   * dimensiones; con el giro, molestaría a la primera.
   */
  actualizar(ficha: Ficha): void {
    const cambioLaForma =
      this.ficha?.ancho !== ficha.ancho || this.ficha?.alto !== ficha.alto;
    this.ficha = ficha;
    // Fuera de un gesto, la geometría la manda el documento. Durante uno, la
    // manda el ratón: si no, el adorno iría un cuadro por detrás del dedo.
    if (!this.gesto) this.degradado = ficha.degradado;
    if (cambioLaForma) this.ajustarAVentana();
    else this.refrescar();
  }

  /** Enseña o esconde las líneas del degradado. Lo decide el panel. */
  ponerAdorno(visible: boolean): void {
    this.verAdorno = visible;
    this.dibujarAdorno();
  }

  descargar(): void {
    this.ficha = null;
    this.degradado = null;
    this.gesto = null;
    this.ctx.clearRect(0, 0, this.canvas.width, this.canvas.height);
    this.dibujarAdorno();
  }

  /** La foto entera dentro de la ventana, con un margen para que respire. */
  ajustarAVentana(): void {
    if (!this.ficha) return;
    const { width, height } = this.nodo.getBoundingClientRect();
    this.zoom = Math.min(width / this.ficha.ancho, height / this.ficha.alto) * 0.95;
    this.cx = this.ficha.ancho / 2;
    this.cy = this.ficha.alto / 2;
    this.refrescar();
  }

  /** Un píxel de la foto en un píxel de la pantalla. */
  unoAUno(): void {
    if (!this.ficha) return;
    this.zoom = 1;
    this.refrescar();
  }

  /**
   * Cambia lo que hace el botón izquierdo.
   *
   * El cursor se cambia con una clase y no con una imagen: `grab` y
   * `crosshair` son cursores que pone el sistema, así que no hay ni un
   * recurso nuevo que cargar ni que escalar en una pantalla con escalado.
   */
  ponerHerramienta(h: Herramienta): void {
    this.herramienta = h;
    this.canvas.classList.toggle("lienzo__canvas--cuentagotas", h === "cuentagotas");
    this.canvas.classList.toggle("lienzo__canvas--degradado", h === "degradado");
    this.dibujarAdorno();
  }

  /**
   * De coordenadas de la foto a píxeles de la ventana, y al revés.
   *
   * Estaban escritas a mano en el manejador de la rueda y en `unCuadro`.
   * Ahora son tres usos reales —la rueda, el cuadro y el gesto del
   * degradado—, que es el umbral a partir del cual este proyecto extrae algo.
   */
  private aPantalla(p: Punto): { x: number; y: number } {
    const r = this.nodo.getBoundingClientRect();
    return {
      x: (p.x - this.cx) * this.zoom + r.width / 2,
      y: (p.y - this.cy) * this.zoom + r.height / 2,
    };
  }

  private aImagen(mx: number, my: number): Punto {
    const r = this.nodo.getBoundingClientRect();
    return {
      x: this.cx + (mx - r.width / 2) / this.zoom,
      y: this.cy + (my - r.height / 2) / this.zoom,
    };
  }

  /**
   * Lee el píxel del lienzo bajo el cursor.
   *
   * # Qué es este número y qué NO es
   *
   * Es el valor **en pantalla**: RGBA de 8 bits del lienzo ya pintado, o sea
   * ya revelado, ya pasado por la curva de sRGB y ya remuestreado al tamaño
   * de la ventana. Dos consecuencias que hay que decir sin rodeos:
   *
   * - No puede contestar «¿le queda detalle a esa nube?». Lo que pasa de 1,0
   *   en luz lineal llega al lienzo recortado a 255, así que ahí el número ya
   *   no distingue. Esa pregunta la contesta el recuento de recorte del
   *   histograma, que está en el panel.
   * - Por debajo del 100 % de zoom es la media de varios píxeles de la foto.
   *
   * Una lectura honrada de la escena necesitaría un comando `muestrear(x, y)`
   * en Rust que leyera la imagen lineal en `f32`. No existe, y por eso la
   * casilla de la barra de estado se llama «En pantalla».
   *
   * Devuelve `null` si se pinchó fuera de la foto: ahí el alfa del lienzo es
   * 0, y enseñar un negro que no está en la imagen sería mentir.
   */
  private muestrear(mx: number, my: number): [number, number, number] | null {
    const dpr = window.devicePixelRatio || 1;
    const x = Math.floor(mx * dpr);
    const y = Math.floor(my * dpr);
    if (x < 0 || y < 0 || x >= this.canvas.width || y >= this.canvas.height) return null;
    // NO se pide `willReadFrequently` al crear el contexto: lo pediría
    // cualquiera al escribir esta línea, y cambiaría el lienzo entero a un
    // camino de software que se pagaría en los milisegundos de cada cuadro,
    // para una lectura de un píxel que ocurre cuando alguien hace clic.
    const d = this.ctx.getImageData(x, y, 1, 1).data;
    if (d[3] === 0) return null;
    return [d[0], d[1], d[2]];
  }

  /** Lee el color bajo el puntero y se lo cuenta a quien escuche. */
  private leerBajoElCursor(e: PointerEvent): void {
    const r = this.nodo.getBoundingClientRect();
    this.alMuestrear?.(this.muestrear(e.clientX - r.left, e.clientY - r.top));
  }

  comparar(activo: boolean): void {
    if (this.comparando === activo) return;
    this.comparando = activo;
    this.dibujarAdorno();
    this.refrescar();
  }

  get estaComparando(): boolean {
    return this.comparando;
  }

  refrescar = porCuadro((): void => {
    void this.pintar();
  });

  private conectarRaton(): void {
    // Zoom con la rueda, anclado al cursor: el punto de la foto que está bajo
    // el ratón se queda bajo el ratón. Sin eso, acercarse a una esquina
    // obliga a arrastrar después, y se nota en seguida que la herramienta es
    // tosca.
    this.canvas.addEventListener(
      "wheel",
      (e) => {
        if (!this.ficha) return;
        e.preventDefault();
        const r = this.nodo.getBoundingClientRect();
        const mx = e.clientX - r.left;
        const my = e.clientY - r.top;

        const ix = this.cx + (mx - r.width / 2) / this.zoom;
        const iy = this.cy + (my - r.height / 2) / this.zoom;

        const factor = Math.exp(-e.deltaY * 0.0015);
        this.zoom = Math.min(ZOOM_MAXIMO, Math.max(ZOOM_MINIMO, this.zoom * factor));

        this.cx = ix - (mx - r.width / 2) / this.zoom;
        this.cy = iy - (my - r.height / 2) / this.zoom;
        this.limitarCentro();
        this.refrescar();
      },
      { passive: false },
    );

    let arrastrando = false;
    let midiendo = false;
    let ultimoX = 0;
    let ultimoY = 0;

    this.canvas.addEventListener("pointerdown", (e) => {
      if (!this.ficha) return;

      // El botón central desplaza SIEMPRE, con cualquier herramienta. No es
      // un extra: es la salida que hace posible que existan modos. Sin ella,
      // con el cuentagotas elegido el arrastre deja de mover la foto y nada
      // en la pantalla lo explica — la herramienta parece rota.
      const desplaza = e.button === 1 || this.herramienta === "mano";

      if (!desplaza) {
        if (e.button === 0 && this.herramienta === "degradado") {
          const r = this.nodo.getBoundingClientRect();
          const mx = e.clientX - r.left;
          const my = e.clientY - r.top;
          const p = this.aImagen(mx, my);
          const cual = this.tiradorBajo(mx, my);
          const d = this.degradado;
          this.gesto =
            cual && d
              ? { cual, desde: d.desde, hasta: d.hasta, origen: p, nacido: true }
              : { cual: "crear", desde: p, hasta: p, origen: p, nacido: false };
          this.canvas.setPointerCapture(e.pointerId);
          return;
        }
        if (e.button === 0) {
          // Se captura el puntero para poder seguir leyendo mientras se
          // arrastra: recorrer la foto con el botón pulsado y ver el color
          // cambiar es lo que convierte el cuentagotas en una herramienta y
          // no en un botón que escribe un número.
          midiendo = true;
          this.canvas.setPointerCapture(e.pointerId);
          this.leerBajoElCursor(e);
        }
        return;
      }

      // Sin esto, el botón central dispara el desplazamiento automático del
      // WebView y aparece el círculo de las cuatro flechas.
      if (e.button === 1) e.preventDefault();

      arrastrando = true;
      ultimoX = e.clientX;
      ultimoY = e.clientY;
      this.canvas.setPointerCapture(e.pointerId);
      this.canvas.classList.add("lienzo__canvas--agarrando");
    });

    this.canvas.addEventListener("pointermove", (e) => {
      if (this.gesto) {
        this.moverGesto(e);
        return;
      }
      if (midiendo) {
        this.leerBajoElCursor(e);
        return;
      }
      if (!arrastrando) return;
      this.cx -= (e.clientX - ultimoX) / this.zoom;
      this.cy -= (e.clientY - ultimoY) / this.zoom;
      ultimoX = e.clientX;
      ultimoY = e.clientY;
      this.limitarCentro();
      this.refrescar();
    });

    const soltar = (e: PointerEvent) => {
      if (this.gesto) {
        const g = this.gesto;
        this.gesto = null;
        this.canvas.releasePointerCapture(e.pointerId);
        if (g.nacido) this.alDegradar?.(g.desde, g.hasta, true);
        return;
      }
      if (midiendo) {
        midiendo = false;
        this.canvas.releasePointerCapture(e.pointerId);
        return;
      }
      if (!arrastrando) return;
      arrastrando = false;
      this.canvas.releasePointerCapture(e.pointerId);
      this.canvas.classList.remove("lienzo__canvas--agarrando");
    };
    this.canvas.addEventListener("pointerup", soltar);
    this.canvas.addEventListener("pointercancel", soltar);

    this.canvas.addEventListener("dblclick", () => this.ajustarAVentana());

    // Escape deshace el gesto en marcha. Sin gesto no hace nada aquí: el
    // lienzo no es quien cierra los menús.
    window.addEventListener("keydown", (e) => {
      if (e.key !== "Escape" || !this.gesto) return;
      const nacido = this.gesto.nacido;
      this.gesto = null;
      this.degradado = this.ficha?.degradado ?? null;
      this.dibujarAdorno();
      if (nacido) this.alDegradar?.(
        this.degradado?.desde ?? { x: 0, y: 0 },
        this.degradado?.hasta ?? { x: 0, y: 0 },
        true,
      );
      e.stopPropagation();
    });
  }

  /** Los tres puntos agarrables, en coordenadas de la foto. */
  private tiradores(): { cual: "desde" | "hasta" | "medio"; p: Punto }[] {
    const d = this.degradado;
    if (!d) return [];
    return [
      { cual: "desde", p: d.desde },
      { cual: "hasta", p: d.hasta },
      {
        cual: "medio",
        p: { x: (d.desde.x + d.hasta.x) / 2, y: (d.desde.y + d.hasta.y) / 2 },
      },
    ];
  }

  /** Cuál de los tres está bajo el puntero, si alguno. */
  private tiradorBajo(mx: number, my: number): "desde" | "hasta" | "medio" | null {
    if (!this.verAdorno) return null;
    for (const t of this.tiradores()) {
      const s = this.aPantalla(t.p);
      if (Math.hypot(s.x - mx, s.y - my) <= RADIO_DE_ACIERTO) return t.cual;
    }
    return null;
  }

  private moverGesto(e: PointerEvent): void {
    const g = this.gesto;
    if (!g) return;
    const r = this.nodo.getBoundingClientRect();
    const mx = e.clientX - r.left;
    const my = e.clientY - r.top;
    let p = this.aImagen(mx, my);

    if (g.cual === "medio") {
      // Mover la franja entera sin tocar ni el ángulo ni la anchura.
      const dx = p.x - g.origen.x;
      const dy = p.y - g.origen.y;
      const base = this.ficha?.degradado;
      if (base) {
        g.desde = { x: base.desde.x + dx, y: base.desde.y + dy };
        g.hasta = { x: base.hasta.x + dx, y: base.hasta.y + dy };
      }
    } else {
      // Con Mayús la dirección se fija al eje más cercano, que es el caso
      // real de un horizonte dos grados torcido.
      const fijo = g.cual === "crear" ? g.desde : g.cual === "desde" ? g.hasta : g.desde;
      if (e.shiftKey) {
        if (Math.abs(p.x - fijo.x) > Math.abs(p.y - fijo.y)) p = { x: p.x, y: fijo.y };
        else p = { x: fijo.x, y: p.y };
      }
      if (g.cual === "desde") g.desde = p;
      else g.hasta = p;
    }

    if (!g.nacido) {
      const a = this.aPantalla(g.desde);
      const b = this.aPantalla(g.hasta);
      if (Math.hypot(b.x - a.x, b.y - a.y) < ARRASTRE_MINIMO) return;
      g.nacido = true;
    }

    // El adorno se dibuja YA, sin esperar a Rust: la línea tiene que ir
    // donde está el ratón mientras la mueves.
    this.degradado = {
      desde: g.desde,
      hasta: g.hasta,
      pasos: this.degradado?.pasos ?? 0,
    };
    this.dibujarAdorno();
    this.alDegradar?.(g.desde, g.hasta, false);
  }

  /**
   * Las dos líneas y los tres tiradores.
   *
   * # La regla de registro
   *
   * Esto se llama en exactamente dos sitios: al final de `unCuadro`, con la
   * misma transformación que generó esos píxeles, y en cada movimiento del
   * ratón mientras hay un gesto. O sea: la línea va donde está el ratón
   * mientras la mueves, y donde está la FOTO en todo lo demás. Si alguien
   * «mejora» esto redibujando también al desplazar, las líneas resbalarán
   * sobre el horizonte y parecerá un error de cálculo.
   */
  private dibujarAdorno(): void {
    const r = this.nodo.getBoundingClientRect();
    const dpr = window.devicePixelRatio || 1;
    const ancho = Math.max(1, Math.round(r.width * dpr));
    const alto = Math.max(1, Math.round(r.height * dpr));
    if (this.adorno.width !== ancho || this.adorno.height !== alto) {
      this.adorno.width = ancho;
      this.adorno.height = alto;
    }
    const c = this.ctxAdorno;
    c.setTransform(dpr, 0, 0, dpr, 0, 0);
    c.clearRect(0, 0, r.width, r.height);

    const d = this.degradado;
    // Mientras se compara hay que ver la foto, no las rayas.
    if (!d || !this.verAdorno || !this.ficha || this.comparando) return;

    const a = this.aPantalla(d.desde);
    const b = this.aPantalla(d.hasta);
    const ex = b.x - a.x;
    const ey = b.y - a.y;
    const largo = Math.hypot(ex, ey) || 1;
    // Perpendicular unitaria, estirada hasta cubrir la ventana entera.
    const px = (-ey / largo) * (r.width + r.height);
    const py = (ex / largo) * (r.width + r.height);

    // Cada trazo se pinta dos veces, negro debajo y blanco encima: una raya
    // de un solo color desaparece sobre un cielo blanco o sobre un suelo
    // negro, que son justo los dos sitios donde se va a usar. Y ni un píxel
    // de color: la interfaz es el fondo contra el que se juzga la foto.
    const trazar = (fn: () => void, discontinua: boolean) => {
      for (const [color, grosor] of [["rgba(0,0,0,0.45)", 3], ["rgba(255,255,255,0.9)", 1.25]] as const) {
        c.strokeStyle = color;
        c.lineWidth = grosor;
        c.setLineDash(discontinua ? [6, 5] : []);
        c.beginPath();
        fn();
        c.stroke();
      }
    };

    trazar(() => {
      c.moveTo(a.x - px, a.y - py);
      c.lineTo(a.x + px, a.y + py);
    }, false);
    trazar(() => {
      c.moveTo(b.x - px, b.y - py);
      c.lineTo(b.x + px, b.y + py);
    }, true);
    c.setLineDash([]);
    c.strokeStyle = "rgba(255,255,255,0.4)";
    c.lineWidth = 1;
    c.beginPath();
    c.moveTo(a.x, a.y);
    c.lineTo(b.x, b.y);
    c.stroke();

    for (const t of this.tiradores()) {
      const s = this.aPantalla(t.p);
      c.beginPath();
      c.arc(s.x, s.y, 5, 0, Math.PI * 2);
      c.fillStyle = "rgba(255,255,255,0.95)";
      c.fill();
      c.strokeStyle = "rgba(0,0,0,0.55)";
      c.lineWidth = 1.25;
      c.stroke();
    }
  }

  /**
   * Impide que la foto se vaya de la ventana.
   *
   * El centro de la vista se queda siempre dentro de la foto. Es la regla más
   * simple que impide perderla de vista, y permite a la vez sacar una esquina
   * hasta la mitad de la pantalla para mirarla cómodo.
   */
  private limitarCentro(): void {
    if (!this.ficha) return;
    this.cx = Math.min(this.ficha.ancho, Math.max(0, this.cx));
    this.cy = Math.min(this.ficha.alto, Math.max(0, this.cy));
  }

  private async pintar(): Promise<void> {
    if (!this.ficha) return;
    if (this.pintando) {
      this.repetir = true;
      return;
    }
    this.pintando = true;
    try {
      do {
        this.repetir = false;
        await this.unCuadro();
      } while (this.repetir);
    } finally {
      this.pintando = false;
    }
  }

  private async unCuadro(): Promise<void> {
    const ficha = this.ficha;
    if (!ficha) return;

    const r = this.nodo.getBoundingClientRect();
    const ancho = Math.floor(r.width);
    const alto = Math.floor(r.height);
    if (ancho <= 0 || alto <= 0) return;

    // Píxeles de dispositivo: en una pantalla con escalado al 150 %, pedir
    // solo los píxeles CSS daría una imagen borrosa justo en la herramienta
    // en la que se mira el enfoque.
    //
    // El tamaño se calcula aquí pero NO se asigna todavía: asignar
    // `canvas.width` borra el bitmap por su cuenta, y hasta que no lleguen
    // los píxeles nuevos no se puede borrar nada.
    const dpr = window.devicePixelRatio || 1;
    const anchoLienzo = Math.round(ancho * dpr);
    const altoLienzo = Math.round(alto * dpr);

    // Dónde cae la foto dentro de la ventana, en píxeles CSS.
    const ix0 = (0 - this.cx) * this.zoom + ancho / 2;
    const iy0 = (0 - this.cy) * this.zoom + alto / 2;
    const ix1 = (ficha.ancho - this.cx) * this.zoom + ancho / 2;
    const iy1 = (ficha.alto - this.cy) * this.zoom + alto / 2;

    // Y qué parte de eso se ve: la intersección con la ventana. Pedirle a
    // Rust solo la intersección es lo que evita tener que inventar un color
    // para lo que queda fuera de la foto; de ese fondo se encarga el CSS.
    const vx0 = Math.max(0, ix0);
    const vy0 = Math.max(0, iy0);
    const vx1 = Math.min(ancho, ix1);
    const vy1 = Math.min(alto, iy1);

    if (vx1 <= vx0 || vy1 <= vy0) {
      // La foto se ha ido entera de la ventana. Este es el único caso en que
      // el lienzo se borra sin tener nada nuevo que poner, y es correcto:
      // aquí lo que hay que enseñar ES el vacío.
      this.redimensionar(anchoLienzo, altoLienzo);
      this.ctx.clearRect(0, 0, this.canvas.width, this.canvas.height);
      this.dibujarAdorno();
      this.alPintar?.({ zoom: this.zoom, ancho: 0, alto: 0, ms: 0 });
      return;
    }

    const destinoAncho = Math.max(1, Math.round((vx1 - vx0) * dpr));
    const destinoAlto = Math.max(1, Math.round((vy1 - vy0) * dpr));

    const comenzo = performance.now();
    let bytes: ArrayBuffer;
    try {
      bytes = await api.renderizar({
        x: (vx0 - ix0) / this.zoom,
        y: (vy0 - iy0) / this.zoom,
        ancho_region: (vx1 - vx0) / this.zoom,
        alto_region: (vy1 - vy0) / this.zoom,
        ancho: destinoAncho,
        alto: destinoAlto,
        con_operaciones: !this.comparando,
      });
    } catch {
      // El documento puede haberse cerrado mientras esta petición viajaba.
      // No es un fallo que haya que enseñar: simplemente ya no hay qué pintar.
      return;
    }
    const ms = performance.now() - comenzo;

    // La foto pudo cerrarse —o abrirse otra— mientras la petición viajaba. Sin
    // esta comprobación, esos píxeles se pintarían encima de un lienzo que ya
    // no es el suyo.
    if (this.ficha !== ficha) return;

    const datos = new ImageData(new Uint8ClampedArray(bytes), destinoAncho, destinoAlto);

    // # Aquí está la razón de que todo lo anterior no tocara el lienzo
    //
    // Borrar y pintar ocurren seguidos, sin un solo `await` en medio. El
    // navegador compone la pantalla entre tareas, nunca a mitad de una, así
    // que es imposible que llegue a enseñar el hueco: lo que había antes
    // sigue a la vista hasta el instante en que lo sustituye lo nuevo.
    //
    // Con el borrado puesto ANTES de pedir los píxeles —que es como estaba—
    // el lienzo pasaba vacío los 30-50 ms de cada petición. Y durante un
    // arrastre eso no se veía como un parpadeo sino como una desaparición
    // completa: el bucle de `pintar` encadena una vuelta tras otra en
    // microtareas, el compositor solo mira entre tareas, y cada vez que
    // miraba el lienzo ya estaba borrado otra vez. La foto no volvía hasta
    // que el usuario soltaba y el último cuadro se quedaba sin sucesor.
    //
    // El precio, que se acepta: mientras se arrastra se ve el cuadro
    // anterior, un paso por detrás, en vez de la posición exacta del ratón.
    // Un retraso se perdona; un hueco, no.
    this.redimensionar(anchoLienzo, altoLienzo);
    this.ctx.clearRect(0, 0, this.canvas.width, this.canvas.height);
    // `putImageData` no pasa por la transformación del contexto, así que sus
    // coordenadas van en píxeles de dispositivo y no en píxeles CSS.
    this.ctx.putImageData(datos, Math.round(vx0 * dpr), Math.round(vy0 * dpr));
    // El adorno, con la MISMA transformación que generó estos píxeles. Por
    // eso va aquí y no en cualquier otro sitio: así nunca se descuadra de la
    // foto que tiene debajo, ni al hacer zoom ni cuando un cuadro llega
    // tarde. Y no hay ningún `await` entre el pintado y esto.
    if (!this.gesto) this.dibujarAdorno();

    this.alPintar?.({ zoom: this.zoom, ancho: destinoAncho, alto: destinoAlto, ms });
  }

  /**
   * Ajusta el bitmap del lienzo, solo si de verdad cambió de tamaño.
   *
   * La comprobación no es por ahorrar: asignar `canvas.width` o
   * `canvas.height` borra el contenido aunque se asigne el mismo número, así
   * que hacerlo en cada cuadro provocaría el mismo hueco que todo lo de
   * arriba trata de evitar.
   */
  private redimensionar(ancho: number, alto: number): void {
    if (this.canvas.width !== ancho || this.canvas.height !== alto) {
      this.canvas.width = ancho;
      this.canvas.height = alto;
    }
  }
}
