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

type Hijos = (Node | string | null | undefined | false)[];

interface Atributos {
  clase?: string;
  texto?: string;
  titulo?: string;
  [atributo: string]: unknown;
}

/** Crea un elemento con sus atributos y sus hijos de una sentada. */
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
