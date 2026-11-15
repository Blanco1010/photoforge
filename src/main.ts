/**
 * PhotoForge: el montaje de la interfaz.
 *
 * Este archivo no sabe nada de fotografía. Sabe qué pieza llama a cuál, qué
 * hay que volver a pintar cuando el documento cambia y cómo se enseña un
 * error. Todo lo demás está en Rust o en las piezas de `ui/`.
 *
 * # Después de la cáscara, exactamente un sitio por acción
 *
 * | Acción                    | Su único sitio | Su tecla   |
 * |---------------------------|----------------|------------|
 * | Abrir                     | Archivo        | Ctrl+O     |
 * | Exportar                  | Archivo        | Ctrl+E     |
 * | Cerrar la foto            | Archivo        | Ctrl+W     |
 * | Salir                     | Archivo        | Ctrl+Q     |
 * | Deshacer / Rehacer        | Editar         | Ctrl+Z / Y |
 * | Restablecer el revelado   | Editar         | —          |
 * | Ajustar a la ventana      | Ver            | 0          |
 * | Tamaño real 1:1           | Ver            | 1          |
 * | Ocultar el panel          | Ver            | P          |
 * | Mano / Cuentagotas        | Herramientas   | M / C      |
 * | Ver el original           | Herramientas   | \ (mantener) |
 *
 * La única puerta doble que queda es el botón «Abrir…» de la pantalla de
 * vacío, y está declarada: no es un control al lado de otro, es el contenido
 * de una pantalla que desaparece en cuanto hay una foto. Dejar al usuario
 * frente a un lienzo gris y un menú que tiene que descubrir sería peor.
 */

import { open, save } from "@tauri-apps/plugin-dialog";
import { getCurrentWindow } from "@tauri-apps/api/window";

import { api, type Ficha, type Formato, type Operacion } from "./lib/api";
import { el } from "./lib/dom";
import { BarraEstado } from "./ui/estado";
import { Herramientas } from "./ui/herramientas";
import { Lienzo } from "./ui/lienzo";
import { coincide, Menu, type GrupoMenu } from "./ui/menu";
import { Panel, PeticionHistograma } from "./ui/panel";

import "./styles/theme.css";
import "./styles/app.css";

/**
 * Los tipos de campo en los que el usuario puede estar escribiendo.
 *
 * Un `input[type=range]` NO cuenta: sus flechas no chocan con ningún atajo, y
 * excluirlo dejaría los deslizadores sin teclado.
 */
const TECLEABLES = new Set(["text", "number", "search", "email", "tel", "url", "password"]);

/** Con coma decimal, igual que la barra de estado. */
function mpx(n: number): string {
  return n.toFixed(1).replace(".", ",");
}

function estaEscribiendo(n: EventTarget | null): boolean {
  if (!(n instanceof HTMLElement)) return false;
  return (
    n.isContentEditable ||
    n instanceof HTMLTextAreaElement ||
    (n instanceof HTMLInputElement && TECLEABLES.has(n.type))
  );
}

class Aplicacion {
  private ficha: Ficha | null = null;
  private readonly lienzo = new Lienzo();
  private readonly panel: Panel;
  private readonly histograma: PeticionHistograma;
  private readonly herramientas = new Herramientas();
  private readonly estado = new BarraEstado();
  private readonly menu: Menu;
  private readonly cuerpo: HTMLElement;
  private readonly vacio: HTMLElement;
  private extensiones: string[] = ["jpg", "jpeg", "png", "tif", "tiff"];
  private panelOculto = false;
  /** La tabla de acciones, construida UNA vez: la comparten menú y teclado. */
  private readonly grupos: GrupoMenu[];

  constructor(raiz: HTMLElement) {
    this.panel = new Panel((op, definitivo) => void this.ajustar(op, definitivo));
    this.histograma = new PeticionHistograma(this.panel);
    this.grupos = this.menus();
    this.menu = new Menu(this.grupos, () => this.ficha);

    this.vacio = el(
      "div",
      { clase: "vacio" },
      el("p", { clase: "vacio__titulo", texto: "Abre una foto para empezar" }),
      el("p", {
        clase: "vacio__ayuda",
        texto: "RAW de cámara, JPG, PNG y TIFF. El archivo original no se modifica nunca.",
      }),
      el("button", {
        clase: "boton boton--principal",
        texto: "Abrir…",
        alClic: () => void this.abrir(),
      }),
    );

    // La barra de estado vive DENTRO de la columna central, debajo del
    // lienzo: no cruza la ventana, igual que en GIMP, y eso es lo que permite
    // que el panel suba hasta debajo de los menús.
    this.cuerpo = el(
      "main",
      { clase: "cuerpo" },
      this.herramientas.nodo,
      el(
        "div",
        { clase: "centro" },
        el("div", { clase: "area" }, this.lienzo.nodo, this.vacio),
        this.estado.nodo,
      ),
      this.panel.nodo,
    );

    raiz.append(this.menu.nodo, this.cuerpo);

    this.lienzo.alPintar = (info) => this.estado.ponerVista(this.ficha ? info : null);
    this.lienzo.alMuestrear = (rgb) => this.estado.ponerColor(rgb);
    this.herramientas.alCambiar = (h) => {
      this.lienzo.ponerHerramienta(h);
      this.estado.ponerHerramienta(h);
    };
    this.herramientas.alComparar = (activo) => {
      this.lienzo.comparar(activo);
      this.estado.comparando(activo);
    };

    this.conectarTeclado();
    this.sincronizar();
    void api.extensiones().then((e) => (this.extensiones = e));
    void this.recuperar();
  }

  /**
   * La tabla de acciones.
   *
   * Es el único punto de esta tanda que hace el código MÁS corto. Antes el
   * mismo dato estaba escrito tres veces —el rótulo del atajo en un botón, la
   * tecla en una escalera de nueve `if`, y la acción en el manejador del
   * clic— y ya había un desajuste real: un botón rotulaba «Ctrl+Y» mientras
   * el teclado atendía además Ctrl+Mayús+Z. Aquí tiene tres consumidores
   * reales: el menú la pinta, el teclado la dispara y el rótulo sale de ella.
   */
  private menus(): GrupoMenu[] {
    const hayFoto = (f: Ficha | null) => f !== null;
    const siempre = () => true;

    return [
      {
        titulo: "Archivo",
        entradas: [
          {
            id: "abrir",
            texto: "Abrir…",
            atajo: { ctrl: true, tecla: "o" },
            activa: siempre,
            hacer: () => void this.abrir(),
          },
          {
            id: "exportar",
            texto: "Exportar como…",
            atajo: { ctrl: true, tecla: "e" },
            separadorAntes: true,
            activa: hayFoto,
            hacer: () => void this.exportar(),
          },
          {
            id: "cerrar",
            texto: "Cerrar",
            atajo: { ctrl: true, tecla: "w" },
            activa: hayFoto,
            hacer: () => void this.cerrarFoto(),
          },
          {
            id: "salir",
            texto: "Salir",
            atajo: { ctrl: true, tecla: "q" },
            separadorAntes: true,
            activa: siempre,
            hacer: () => void getCurrentWindow().close(),
          },
        ],
      },
      {
        titulo: "Editar",
        entradas: [
          {
            id: "deshacer",
            texto: "Deshacer",
            atajo: { ctrl: true, tecla: "z" },
            activa: (f) => !!f?.puede_deshacer,
            hacer: () => void this.paso(api.deshacer),
          },
          {
            id: "rehacer",
            texto: "Rehacer",
            atajo: { ctrl: true, tecla: "y" },
            // El que también dispara y no se enseña. Declararlo aquí, al lado
            // del que se rotula, es lo que impide que vuelvan a separarse.
            alias: { ctrl: true, mayus: true, tecla: "z" },
            activa: (f) => !!f?.puede_rehacer,
            hacer: () => void this.paso(api.rehacer),
          },
          {
            // «Restablecer el revelado» y no «Restablecer» a secas: en un
            // menú, sin los botones vecinos que le daban contexto, no se
            // sabría qué se restablece.
            //
            // Y NO se llama «Revertir» ni vive en Archivo, aunque ahí es
            // donde lo pone GIMP: allí Revertir vuelve a leer el archivo del
            // disco, y aquí el original nunca salió de la memoria. Usar la
            // palabra de GIMP para un acto distinto sería la fidelidad que
            // miente.
            id: "restablecer",
            texto: "Restablecer el revelado",
            separadorAntes: true,
            activa: (f) => !!f?.hay_cambios,
            hacer: () => void this.paso(api.restablecer),
          },
        ],
      },
      {
        titulo: "Ver",
        entradas: [
          {
            id: "ajustar",
            texto: "Ajustar a la ventana",
            atajo: { tecla: "0" },
            activa: hayFoto,
            hacer: () => this.lienzo.ajustarAVentana(),
          },
          {
            id: "unoauno",
            texto: "Tamaño real 1:1",
            atajo: { tecla: "1" },
            activa: hayFoto,
            hacer: () => this.lienzo.unoAUno(),
          },
          {
            // El texto se da la vuelta al abrir el menú en vez de dibujar una
            // marca de verificación: una línea contra diez, y dice más.
            id: "panel",
            texto: () => (this.panelOculto ? "Enseñar el panel" : "Ocultar el panel"),
            // La tecla es P y NO Tab. Tab es la única cuyo comportamiento por
            // omisión esta interfaz necesita: los deslizadores son los únicos
            // controles a los que hoy se llega con el teclado, y robándolo
            // dejarían de ser alcanzables.
            atajo: { tecla: "p" },
            separadorAntes: true,
            activa: siempre,
            hacer: () => this.alternarPanel(),
          },
        ],
      },
    ];
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
    this.estado.decir(`${this.ficha.nombre} · ${mpx(this.ficha.megapixeles)} Mpx`);
  }

  private sincronizar(): void {
    this.herramientas.sincronizar(this.ficha);
    this.panel.sincronizar(this.ficha);
    this.panel.habilitar(this.ficha !== null);
    this.estado.ponerFoto(this.ficha);
    this.vacio.hidden = this.ficha !== null;
    this.lienzo.nodo.hidden = this.ficha === null;

    // El asterisco es el de GIMP. Solo el nombre y el asterisco: Windows
    // recorta el título en la barra de tareas, y un título corto es lo que
    // mantiene el asterisco a la vista.
    const titulo = this.ficha
      ? `${this.ficha.hay_cambios ? "*" : ""}${this.ficha.nombre} — PhotoForge`
      : "PhotoForge";
    void getCurrentWindow().setTitle(titulo);
  }

  /**
   * Cierra la foto y suelta sus píxeles.
   *
   * El comando existía en Rust desde el principio y no tenía ninguna puerta
   * en la interfaz. Importa más de lo que parece: una foto de 24 Mpx son 280
   * MB, y sin esto solo se liberan al abrir la siguiente.
   */
  private async cerrarFoto(): Promise<void> {
    if (!this.ficha) return;
    // Por si se estaba comparando: si no, el lienzo se quedaría enseñando
    // el original de una foto que ya no está.
    this.lienzo.comparar(false);
    this.estado.comparando(false);
    await api.cerrar();
    this.ficha = null;
    this.lienzo.descargar();
    // Y el histograma, que si no se queda enseñando el de una foto que ya no
    // existe.
    this.panel.ponerHistograma(null, false);
    this.sincronizar();
    this.estado.decir("Listo");
  }

  /**
   * Esconde el panel para darle al lienzo la ventana entera.
   *
   * Es distinto de plegar una sección: plegar reorganiza el panel, esconderlo
   * le da la pantalla a la imagen. El lienzo se entera solo porque ya tiene
   * su `ResizeObserver`.
   *
   * No se guarda entre sesiones: es un gesto momentáneo, y un panel
   * escondido al arrancar parece una ventana rota.
   */
  private alternarPanel(): void {
    this.panelOculto = !this.panelOculto;
    this.cuerpo.classList.toggle("cuerpo--sin-panel", this.panelOculto);
  }

  private async abrir(): Promise<void> {
    const elegido = await open({
      multiple: false,
      directory: false,
      filters: [{ name: "Fotografías", extensions: this.extensiones }],
    });
    if (typeof elegido !== "string") return;

    this.estado.decir("Abriendo…");
    try {
      this.ficha = await api.abrir(elegido);
      this.sincronizar();
      this.lienzo.cargar(this.ficha);
      await this.histograma.pedir(true);
      this.estado.decir(`${this.ficha.nombre} · ${mpx(this.ficha.megapixeles)} Mpx`);
    } catch (e) {
      this.ficha = null;
      this.lienzo.descargar();
      this.sincronizar();
      this.estado.decir(String(e), true);
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
      this.herramientas.sincronizar(this.ficha);
      this.lienzo.refrescar();
      if (definitivo) this.sincronizar();
      await this.histograma.pedir(definitivo);
    } catch (e) {
      this.estado.decir(String(e), true);
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
      this.estado.decir(String(e), true);
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

    this.estado.decir("Exportando a resolución completa…");
    try {
      const escrito = await api.exportar(destino, formato);
      this.estado.decir(`Exportado: ${escrito}`);
    } catch (e) {
      this.estado.decir(String(e), true);
    }
  }

  /**
   * Los atajos globales.
   *
   * Esto era una escalera de nueve `if` con los atajos escritos a mano; ahora
   * es un recorrido de la tabla. Nótese que NO hace falta consultar si hay un
   * menú abierto: la barra se queda con el teclado mientras lo tiene, y este
   * escucha cuelga de `window`, que está por encima en el árbol.
   */
  private conectarTeclado(): void {
    const acciones = this.grupos.flatMap((g) => g.entradas);

    window.addEventListener("keydown", (e) => {
      const ctrl = e.ctrlKey || e.metaKey;

      // Los de una sola tecla se guardan de los campos de texto; los que
      // llevan Ctrl, no: Ctrl+Z dentro de un campo debe seguir deshaciendo el
      // revelado.
      if (!ctrl && estaEscribiendo(e.target)) return;

      for (const a of acciones) {
        if (coincide(e, a.atajo) || coincide(e, a.alias)) {
          if (!a.activa(this.ficha)) return;
          e.preventDefault();
          a.hacer();
          return;
        }
      }

      if (this.ficha && this.herramientas.atajo(e)) e.preventDefault();
    });

    // El fin de un gesto no se filtra NUNCA. Si el `keyup` de `\` se perdiera,
    // el lienzo se quedaría enseñando el original para siempre, porque el
    // gesto no tendría quien lo terminara.
    window.addEventListener("keyup", (e) => void this.herramientas.atajoSoltar(e));
  }
}

const raiz = document.getElementById("app");
if (!raiz) throw new Error("falta el <div id=\"app\"> en index.html");
new Aplicacion(raiz);
