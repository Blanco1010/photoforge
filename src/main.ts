/**
 * PhotoForge: el montaje de la interfaz.
 *
 * Este archivo no sabe nada de fotografía. Sabe qué pieza llama a cuál, qué
 * hay que volver a pintar cuando el documento cambia y cómo se enseña un
 * error. Todo lo demás está en Rust o en las piezas de `ui/`.
 */

import { open, save } from "@tauri-apps/plugin-dialog";

import { api, type Ficha, type Formato, type Operacion } from "./lib/api";
import { el, vaciar } from "./lib/dom";
import { Barra } from "./ui/barra";
import { Lienzo } from "./ui/lienzo";
import { Panel, PeticionHistograma } from "./ui/panel";

import "./styles/theme.css";
import "./styles/app.css";

class Aplicacion {
  private ficha: Ficha | null = null;
  private readonly lienzo = new Lienzo();
  private readonly panel: Panel;
  private readonly histograma: PeticionHistograma;
  private readonly barra: Barra;
  private readonly estado: HTMLElement;
  private readonly vacio: HTMLElement;
  private extensiones: string[] = ["jpg", "jpeg", "png", "tif", "tiff"];

  constructor(raiz: HTMLElement) {
    this.panel = new Panel((op, definitivo) => void this.ajustar(op, definitivo));
    this.histograma = new PeticionHistograma(this.panel);
    this.barra = new Barra({
      abrir: () => void this.abrir(),
      deshacer: () => void this.paso(api.deshacer),
      rehacer: () => void this.paso(api.rehacer),
      restablecer: () => void this.paso(api.restablecer),
      exportar: () => void this.exportar(),
      ajustarAVentana: () => this.lienzo.ajustarAVentana(),
      unoAUno: () => this.lienzo.unoAUno(),
      comparar: (activo) => this.lienzo.comparar(activo),
    });

    this.estado = el("footer", { clase: "estado" }, el("span", { texto: "Listo" }));
    this.vacio = el(
      "div",
      { clase: "vacio" },
      el("p", { clase: "vacio__titulo", texto: "Abre una foto para empezar" }),
      el("p", {
        clase: "vacio__ayuda",
        texto: "JPG, PNG y TIFF. El archivo original no se modifica nunca.",
      }),
      el("button", {
        clase: "boton boton--principal",
        texto: "Abrir…",
        alClic: () => void this.abrir(),
      }),
    );

    raiz.append(
      this.barra.nodo,
      el("main", { clase: "cuerpo" }, el("div", { clase: "area" }, this.lienzo.nodo, this.vacio), this.panel.nodo),
      this.estado,
    );

    this.lienzo.alPintar = (info) => {
      if (!this.ficha) return;
      const zoom = (info.zoom * 100).toFixed(info.zoom < 1 ? 1 : 0);
      this.decir(
        `Zoom ${zoom} % · ${info.ancho} × ${info.alto} px calculados en ${info.ms.toFixed(0)} ms` +
          (this.lienzo.estaComparando ? " · enseñando el original" : ""),
      );
    };

    this.conectarTeclado();
    this.sincronizar();
    void api.extensiones().then((e) => (this.extensiones = e));
    void this.recuperar();
  }

  /**
   * Recupera el documento que Rust ya tuviera abierto.
   *
   * Hace falta porque la interfaz y el documento viven en sitios distintos:
   * los píxeles están en Rust y la ventana es una página web. Durante el
   * desarrollo, cualquier cambio en un `.ts` recarga la página, y sin esto la
   * foto desaparecía de la pantalla mientras sus 142 MB seguían ocupados en
   * memoria. Al arrancar de cero no hay documento, el comando falla y no pasa
   * nada: esa es la rama normal.
   */
  private async recuperar(): Promise<void> {
    try {
      this.ficha = await api.ficha();
    } catch {
      return;
    }
    this.sincronizar();
    this.lienzo.cargar(this.ficha);
    await this.histograma.pedir(true);
    this.decir(`${this.ficha.nombre} · ${this.ficha.megapixeles.toFixed(1)} Mpx`);
  }

  private decir(texto: string, esError = false): void {
    vaciar(this.estado);
    this.estado.append(
      el("span", { clase: esError ? "estado__error" : "", texto }),
    );
  }

  private sincronizar(): void {
    this.barra.sincronizar(this.ficha);
    this.panel.sincronizar(this.ficha);
    this.panel.habilitar(this.ficha !== null);
    this.vacio.hidden = this.ficha !== null;
    this.lienzo.nodo.hidden = this.ficha === null;
  }

  private async abrir(): Promise<void> {
    const elegido = await open({
      multiple: false,
      directory: false,
      filters: [{ name: "Fotografías", extensions: this.extensiones }],
    });
    if (typeof elegido !== "string") return;

    this.decir("Abriendo…");
    try {
      this.ficha = await api.abrir(elegido);
      this.sincronizar();
      this.lienzo.cargar(this.ficha);
      await this.histograma.pedir(true);
      this.decir(`${this.ficha.nombre} · ${this.ficha.megapixeles.toFixed(1)} Mpx`);
    } catch (e) {
      this.ficha = null;
      this.lienzo.descargar();
      this.sincronizar();
      this.decir(String(e), true);
    }
  }

  /**
   * Un cambio de deslizador.
   *
   * `definitivo` distingue arrastrar de soltar. Mientras se arrastra basta con
   * el histograma rápido; al soltar se pide el completo, que es el único que
   * puede contar bien el recorte. Hacer el completo en cada movimiento sería
   * recorrer la foto entera sesenta veces por segundo para enseñar un número
   * que el usuario no va a leer hasta que suelte.
   */
  private async ajustar(op: Operacion, definitivo: boolean): Promise<void> {
    if (!this.ficha) return;
    try {
      this.ficha = await api.ajustar(op);
      this.barra.sincronizar(this.ficha);
      this.lienzo.refrescar();
      await this.histograma.pedir(definitivo);
    } catch (e) {
      this.decir(String(e), true);
    }
  }

  /** Deshacer, rehacer y restablecer: los tres hacen lo mismo con la ficha. */
  private async paso(accion: () => Promise<Ficha>): Promise<void> {
    if (!this.ficha) return;
    try {
      this.ficha = await accion();
      this.sincronizar();
      this.lienzo.refrescar();
      await this.histograma.pedir(true);
    } catch (e) {
      this.decir(String(e), true);
    }
  }

  private async exportar(): Promise<void> {
    if (!this.ficha) return;
    const base = this.ficha.nombre.replace(/\.[^.]+$/, "");
    const destino = await save({
      defaultPath: `${base}-revelado.jpg`,
      filters: [
        { name: "JPEG (para compartir)", extensions: ["jpg", "jpeg"] },
        { name: "PNG (sin pérdida, 8 bits)", extensions: ["png"] },
        { name: "TIFF (sin pérdida, 16 bits)", extensions: ["tif", "tiff"] },
      ],
    });
    if (!destino) return;

    // El formato sale de la extensión que el usuario escribió y no de un
    // desplegable aparte: el nombre del archivo y su contenido no se pueden
    // contradecir, y la extensión es lo que el usuario cree que ha elegido.
    const ext = destino.split(".").pop()?.toLowerCase() ?? "jpg";
    let formato: Formato;
    if (ext === "png") formato = { tipo: "png" };
    else if (ext === "tif" || ext === "tiff") formato = { tipo: "tiff16" };
    else formato = { tipo: "jpeg", calidad: 92 };

    this.decir("Exportando a resolución completa…");
    try {
      const escrito = await api.exportar(destino, formato);
      this.decir(`Exportado: ${escrito}`);
    } catch (e) {
      this.decir(String(e), true);
    }
  }

  private conectarTeclado(): void {
    window.addEventListener("keydown", (e) => {
      const ctrl = e.ctrlKey || e.metaKey;
      if (ctrl && e.key.toLowerCase() === "o") {
        e.preventDefault();
        void this.abrir();
      } else if (ctrl && e.key.toLowerCase() === "e") {
        e.preventDefault();
        void this.exportar();
      } else if (ctrl && e.key.toLowerCase() === "z" && !e.shiftKey) {
        e.preventDefault();
        void this.paso(api.deshacer);
      } else if (ctrl && (e.key.toLowerCase() === "y" || (e.shiftKey && e.key.toLowerCase() === "z"))) {
        e.preventDefault();
        void this.paso(api.rehacer);
      } else if (!ctrl && e.key === "0") {
        this.lienzo.ajustarAVentana();
      } else if (!ctrl && e.key === "1") {
        this.lienzo.unoAUno();
      } else if (!ctrl && e.key === "\\" && !e.repeat) {
        this.lienzo.comparar(true);
      }
    });
    window.addEventListener("keyup", (e) => {
      if (e.key === "\\") this.lienzo.comparar(false);
    });
  }
}

const raiz = document.getElementById("app");
if (!raiz) throw new Error("falta el <div id=\"app\"> en index.html");
new Aplicacion(raiz);
