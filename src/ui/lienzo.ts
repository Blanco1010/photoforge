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

import { api, type Ficha } from "../lib/api";
import { el, porCuadro } from "../lib/dom";
import type { Herramienta } from "./herramientas";

export interface InfoVista {
  zoom: number;
  ancho: number;
  alto: number;
  ms: number;
}

/** Hasta dónde se puede ampliar. 32× ya enseña el píxel como un cuadrado. */
const ZOOM_MAXIMO = 32;
/** Y hasta dónde alejar: por debajo de esto no se distingue nada. */
const ZOOM_MINIMO = 0.02;

export class Lienzo {
  readonly nodo: HTMLDivElement;
  private readonly canvas: HTMLCanvasElement;
  private readonly ctx: CanvasRenderingContext2D;

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

  constructor() {
    this.canvas = el("canvas", { clase: "lienzo__canvas" });
    this.nodo = el("div", { clase: "lienzo" }, this.canvas);
    const ctx = this.canvas.getContext("2d", { alpha: true });
    if (!ctx) throw new Error("este WebView no da un contexto 2D");
    this.ctx = ctx;

    this.conectarRaton();
    new ResizeObserver(porCuadro(() => this.refrescar())).observe(this.nodo);
  }

  cargar(ficha: Ficha): void {
    this.ficha = ficha;
    this.ajustarAVentana();
  }

  descargar(): void {
    this.ficha = null;
    this.ctx.clearRect(0, 0, this.canvas.width, this.canvas.height);
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

  comparar(activo: boolean): void {
    if (this.comparando === activo) return;
    this.comparando = activo;
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
        if (e.button === 0) {
          const r = this.nodo.getBoundingClientRect();
          this.alMuestrear?.(this.muestrear(e.clientX - r.left, e.clientY - r.top));
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
      if (!arrastrando) return;
      this.cx -= (e.clientX - ultimoX) / this.zoom;
      this.cy -= (e.clientY - ultimoY) / this.zoom;
      ultimoX = e.clientX;
      ultimoY = e.clientY;
      this.limitarCentro();
      this.refrescar();
    });

    const soltar = (e: PointerEvent) => {
      if (!arrastrando) return;
      arrastrando = false;
      this.canvas.releasePointerCapture(e.pointerId);
      this.canvas.classList.remove("lienzo__canvas--agarrando");
    };
    this.canvas.addEventListener("pointerup", soltar);
    this.canvas.addEventListener("pointercancel", soltar);

    this.canvas.addEventListener("dblclick", () => this.ajustarAVentana());
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
