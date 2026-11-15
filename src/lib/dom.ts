/**
 * Cuatro ayudas para construir HTML sin framework.
 *
 * Por qué no hay framework: PhotoForge tiene una pantalla. Toda su
 * complejidad está en el lienzo —que se pinta con `putImageData` y al que un
 * DOM virtual no le aporta nada— y en Rust. Un framework aquí añadiría un paso
 * de compilación, un modelo mental y doscientos kilobytes para resolver un
 * problema que no se tiene.
 *
 * Lo que sí hace falta es no escribir `document.createElement` cuarenta veces,
 * y eso cabe en este archivo.
 */

export type Hijos = (Node | string | null | undefined | false)[];

interface Atributos {
  clase?: string;
  texto?: string;
  titulo?: string;
  [atributo: string]: unknown;
}

/**
 * Crea un elemento con sus atributos y sus hijos de una sentada.
 *
 * Para un SVG NO sirve: usa `icono()`, que está justo debajo y explica por qué.
 */
export function el<K extends keyof HTMLElementTagNameMap>(
  etiqueta: K,
  atributos: Atributos = {},
  ...hijos: Hijos
): HTMLElementTagNameMap[K] {
  const nodo = document.createElement(etiqueta);
  for (const [clave, valor] of Object.entries(atributos)) {
    if (valor === undefined || valor === null || valor === false) continue;
    if (clave === "clase") nodo.className = String(valor);
    else if (clave === "texto") nodo.textContent = String(valor);
    else if (clave === "titulo") nodo.title = String(valor);
    else if (clave.startsWith("al")) {
      // alClic -> click, alEntrada -> input. Los nombres van en castellano
      // como todo lo demás, y la traducción vive en un solo sitio.
      const eventos: Record<string, string> = {
        alClic: "click",
        alEntrada: "input",
        alCambio: "change",
        alSoltar: "pointerup",
      };
      const evento = eventos[clave];
      if (evento) nodo.addEventListener(evento, valor as EventListener);
    } else if (clave in nodo) {
      (nodo as unknown as Record<string, unknown>)[clave] = valor;
    } else {
      nodo.setAttribute(clave, String(valor));
    }
  }
  for (const hijo of hijos) {
    if (hijo === null || hijo === undefined || hijo === false) continue;
    nodo.append(hijo);
  }
  return nodo;
}

/**
 * Un icono SVG dibujado a partir de sus rutas.
 *
 * # Por qué esto no se puede hacer con `el()`
 *
 * `el()` llama a `document.createElement`, y `document.createElement("svg")`
 * NO devuelve un SVG: devuelve un `HTMLUnknownElement` que no pinta nada, sin
 * error y sin un aviso en la consola. Los iconos saldrían invisibles y se iría
 * la tarde buscando el fallo en el CSS. Un SVG necesita `createElementNS` con
 * su espacio de nombres, y por eso esta función vive aquí al lado de la trampa
 * en vez de perdida en el archivo que dibuja los iconos.
 *
 * `trazo` es la ruta que va dibujada con línea, y puede llevar varias
 * subrutas. `relleno` es la ruta opcional que va maciza.
 */
export function icono(trazo: string, relleno?: string): SVGSVGElement {
  const NS = "http://www.w3.org/2000/svg";
  const svg = document.createElementNS(NS, "svg");
  svg.setAttribute("viewBox", "0 0 24 24");
  svg.setAttribute("width", "20");
  svg.setAttribute("height", "20");
  svg.setAttribute("aria-hidden", "true");

  if (relleno) {
    const macizo = document.createElementNS(NS, "path");
    macizo.setAttribute("d", relleno);
    macizo.setAttribute("fill", "currentColor");
    macizo.setAttribute("opacity", "0.45");
    svg.append(macizo);
  }

  const linea = document.createElementNS(NS, "path");
  linea.setAttribute("d", trazo);
  linea.setAttribute("fill", "none");
  linea.setAttribute("stroke", "currentColor");
  linea.setAttribute("stroke-width", "1.6");
  linea.setAttribute("stroke-linecap", "round");
  linea.setAttribute("stroke-linejoin", "round");
  svg.append(linea);

  return svg;
}

/** Vacía un nodo. */
export function vaciar(nodo: HTMLElement): void {
  while (nodo.firstChild) nodo.removeChild(nodo.firstChild);
}

/**
 * Agrupa las llamadas que ocurran en el mismo cuadro de animación.
 *
 * Arrastrar un deslizador dispara eventos más rápido de lo que la pantalla se
 * refresca. Sin esto se pedirían diez renders para enseñar uno, y los nueve
 * sobrantes no solo se tiran: ocupan el hilo que tenía que calcular el bueno.
 */
export function porCuadro<T extends unknown[]>(f: (...args: T) => void): (...args: T) => void {
  let pendiente = false;
  let ultimos: T;
  return (...args: T) => {
    ultimos = args;
    if (pendiente) return;
    pendiente = true;
    requestAnimationFrame(() => {
      pendiente = false;
      f(...ultimos);
    });
  };
}

/** Espera a que pase un rato sin llamadas. Para lo que es caro de verdad. */
export function trasPausa<T extends unknown[]>(
  ms: number,
  f: (...args: T) => void,
): (...args: T) => void {
  let temporizador: number | undefined;
  return (...args: T) => {
    if (temporizador !== undefined) clearTimeout(temporizador);
    temporizador = window.setTimeout(() => f(...args), ms);
  };
}
