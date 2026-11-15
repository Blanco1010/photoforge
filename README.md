![PhotoForge](docs/imagenes/ventana.png)

# PhotoForge

Editor de fotografía no destructivo para Windows. Abre una foto, la revela sin
tocar nunca el archivo original y la exporta.

Escrito en **Rust** con **Tauri 2**. El núcleo de imagen es Rust puro y la
interfaz es TypeScript sin framework.

Este README explica **por qué** está hecho así. El qué se lee en el código.

---

## Qué hace hoy

Abre **RAW de cámara** (DNG, CR2, CR3, NEF, ARW, RAF, RW2, ORF y más), JPG,
PNG y TIFF de 8 o 16 bits. Los pinta en un lienzo con zoom y desplazamiento,
enseña sus metadatos EXIF y su histograma, aplica exposición, curva de luces y
contraste de forma no destructiva, compara con cómo se abrió manteniendo
pulsado un botón, deshace y rehace, y exporta a JPEG, PNG o TIFF de 16 bits.

Lo que **no** hace todavía: capas, máscaras, selecciones y guardar proyectos.
El apartado [Lo que falta](#lo-que-falta) dice en qué orden y por qué.

---

## La prueba de riesgo: ¿se puede decodificar RAW en Rust, en Windows?

Todo el proyecto depende de esa respuesta, así que fue lo primero que se
comprobó, antes de escribir una línea de la aplicación. Medido el **14 de
noviembre de 2026** en un Windows 11 con el toolchain de MSVC.

### Lo que hay de verdad en crates.io

| Biblioteca | Versión | Publicada | Licencia | Estado |
|---|---|---|---|---|
| [`rawler`](https://crates.io/crates/rawler) | 0.8.0 | 2026-08-30 | LGPL-2.1 | **La elegida.** Rust puro. 227.000 descargas. Es el motor de [DNGLab](https://github.com/dnglab/dnglab), que convierte RAW a DNG desde 2021 |
| [`flashraw`](https://crates.io/crates/flashraw) | 0.3.0 | 2026-11-14 | MIT/Apache-2.0 | Prometedora y demasiado nueva. Nació el 25 de octubre de 2026 y **las tres versiones anteriores están retiradas**. 2.000 descargas |
| [`rawloader`](https://crates.io/crates/rawloader) | 0.37.2 | 2026-08-15 | LGPL-2.1 | El antecesor de `rawler`, del mismo entorno. Menos formatos |
| [`libraw-sys`](https://crates.io/crates/libraw-sys) | 0.1.1 | **2015-11-05** | — | Abandonada hace once años. Depende de la caja `gcc`, que ya ni existe con ese nombre, y busca la biblioteca con `pkg-config`: en Windows no hay por dónde cogerla |

Las dos candidatas **compilan en Windows con MSVC sin pelearse**: son Rust
puro, no hace falta vendorizar LibRaw, ni `pkg-config`, ni una DLL al lado del
ejecutable. Esa era la pregunta de verdad, y la respuesta es que sí.

### Qué pasa con el DNG del Samsung Galaxy S25 Ultra

Medido sobre cinco archivos de Expert RAW, de 13 a 23 MB:

```text
samsung Galaxy S25 Ultra
  decodificado en 504 ms
  4080×3060, 3 componentes por píxel, 16 bits
  mosaico: NO, el archivo ya viene lineal — demosaico sería un error
  niveles de negro: 12 valores (patrón 2×2, 3 canales)
  niveles de blanco: [65535, 65535, 65535]
```

Confirmado: **el DNG de Expert RAW es lineal de 16 bits y no lleva mosaico de
Bayer**. `rawler` lo marca como `LinearRaw` y su propio revelador se salta el
demosaico sin que haya que decirle nada, así que el riesgo de destrozar el
archivo aplicándole un demosaico que no toca no existe.

### Y el fallo que apareció por el camino

El primer revelado salió **enteramente blanco**. La causa no era la
decodificación:

`RawImage::apply_scaling`, que es lo que normaliza los valores a 0..1, llama a
`correct_blacklevel(datos, negros, blancos)`. Esa función compara
`negros.len()` con `blancos.len()` antes de hacer nada. El DNG del S25 Ultra
trae **12 niveles de negro** (un patrón de 2×2 por 3 canales) y **3 de
blanco**. Como 12 ≠ 3, la función cae en su rama final, escribe un
`log::warn!` y **vuelve sin tocar un solo píxel**.

Nadie ve ese aviso: una aplicación de escritorio sin `env_logger` montado no lo
saca por ningún sitio. Los valores siguen entre 0 y 65.535 cuando el resto de
la cadena los espera entre 0 y 1, y al pintar todo lo que pasa de 1 se recorta
a blanco. Los cinco archivos de prueba dan 12 contra 3, así que les pasa a
todos.

Por eso PhotoForge **normaliza por su cuenta**, aplicando el patrón de negro en
mosaico, y no se apoya en `apply_scaling`. Está en
[`crates/pf-image/examples/sonda_raw.rs`](crates/pf-image/examples/sonda_raw.rs),
con el porqué escrito al lado del código.

### El nivel de blanco declarado no es el de la escena

Máximos crudos de los cinco archivos: **1.334, 2.139, 3.834, 9.214 y 29.352**,
sobre un nivel de blanco declarado de 65.535. Varían con la escena, así que
65.535 sí es el techo bueno: esas fotos sencillamente no llegan a quemarse.

La consecuencia práctica: **en luz lineal, una foto bien expuesta se ve
oscura.** No es un fallo de la normalización, es lo que significa trabajar en
lineal, y es el motivo de que un revelador necesite una curva de tono. El
ejemplo lo avisa cuando detecta el caso en vez de dejar que parezca un error.

### Correr la sonda

```bash
cargo run -p pf-image --example sonda_raw --release -- foto.dng salida.tif
```

Informa de lo que trae el archivo, lo normaliza, lo revela a RGB lineal y
escribe un TIFF de 16 bits pasando por los mismos tipos que usa la aplicación.

### Por qué `rawler` aunque sea LGPL-2.1

PhotoForge es MIT y `rawler` es LGPL-2.1. La LGPL obliga a que quien reciba el
binario pueda reenlazarlo contra otra versión de la biblioteca; con un
ejecutable de Rust, que enlaza estáticamente, eso significaría entregar los
objetos. Aquí se cumple sola porque **el código fuente entero está publicado y
cualquiera puede recompilar**.

Si algún día PhotoForge se vendiera cerrado, esto habría que rehacerlo: o
`flashraw` (MIT, mucho más verde) o `rawler` en un proceso aparte. Queda dicho
para que la decisión no sorprenda a nadie más adelante.

---

## Revelar un RAW

Un RAW no se puede «enseñar»: hay que revelarlo. Estos son los tres pasos que
PhotoForge le hace a un archivo de cámara nada más abrirlo, y por qué.

### 1. Normalizar a mano, porque `rawler` no lo hace con estos archivos

Está contado arriba, en la prueba de riesgo. El resumen: 12 niveles de negro
contra 3 de blanco, la comprobación de longitudes no cuadra y la normalización
se salta en silencio. Lo hace
[`pf_image::raw::normalizar`](crates/pf-image/src/raw.rs), que además mira
antes si ya estaba hecha: si algún día arreglan `rawler`, esto no normalizará
dos veces, y no por acordarse sino por construcción.

### 2. Una exposición de partida, calculada y a la vista

Medido sobre cinco DNG del S25 Ultra, los valores crudos ocupan entre el 2 % y
el 45 % del techo del sensor. Enseñar eso tal cual da una foto oscura, y
cualquiera pensaría que algo se rompió en vez de que falta revelarla.

Así que al abrir se calcula cuánta exposición hace falta. **Con un percentil
alto y no con la media**: la media la arrastra lo que más superficie ocupa, así
que una foto con fondo negro saldría quemada y una con nieve, gris. El
percentil 99,5 mira dónde están las luces, que es lo que no se puede
recuperar, y deja fuera el 0,5 % más brillante a propósito — una farola o un
reflejo DEBEN quemarse, y si contaran bastaría uno para dejar la foto entera
subexpuesta.

Lo que sale de ahí **no es magia escondida**: entra en la lista de operaciones
como una exposición normal, aparece en su deslizador y se puede mover o
quitar. Sobre las cinco fotos de prueba salen entre +1,06 y +5,78 pasos, y
ninguna de las cinco queda con un solo píxel quemado.

### 3. Doblar las luces en vez de cortarlas

Al subir cinco pasos de exposición, todo lo que se pasa de 1,0 se recortaría:
un cielo con nubes se convertiría en una mancha blanca plana. La curva de
luces hace lo que hacía la película —acercarse al blanco sin llegar—, con dos
condiciones que la hacen honrada:

- **Por debajo de 0,75 no toca absolutamente nada.** Las curvas tipo Reinhard
  comprimen también los medios tonos, y entonces activarlas oscurece una foto
  que ya estaba bien. Con un codo, activar la curva no puede estropear la
  parte de la foto que te gustaba.
- **Encaja con pendiente 1 justo en el codo**, así que no hay esquina ni se ve
  una banda en la transición.

Es una casilla y no un deslizador porque lo único regulable sería dónde está
el codo, y eso no se puede juzgar mirando la foto: el efecto solo ocurre en el
puñado de píxeles más brillantes. Un control que no se puede juzgar es un
control que nadie mueve.

Lo que la curva **no** puede hacer está escrito igual de claro en el código:
en `f32` deja de distinguir diferencias finas por encima de 3,1 y satura en
1,0 a partir de 4,7. Salva un cielo sobreexpuesto un par de pasos; lo que esté
por encima ya no cabe en el número.

### «Antes» es como se abrió, no la imagen cruda

Lo evidente sería que el botón de comparar enseñara la foto sin ninguna
operación. Para un JPEG lo es. Para un RAW no: sin operaciones, un RAW son
fotones medidos y se ve casi negro, así que comparar contra eso no enseña lo
que acabas de hacer — enseña lo que hace un RAW sin revelar.

Así que «Antes» es **como se abrió la foto**, con su exposición de partida y
su curva. Es exactamente lo mismo contra lo que vuelve «Restablecer»: si las
dos cosas significaran cosas distintas, el programa se estaría contradiciendo.

### Y la foto sale derecha

Una cámara de móvil no gira los píxeles cuando giras el teléfono: los guarda
siempre igual y apunta aparte cómo hay que verlos. Ni `rawler` ni la caja
`image` aplican ese dato, así que sin hacerlo la mitad de las fotos salen
tumbadas — es el fallo que apareció en cuanto se abrió la segunda foto de
prueba.

PhotoForge lo aplica **al abrir**, y a partir de ahí el resto del programa
trabaja con una imagen que ya está derecha. La alternativa —arrastrar la
orientación por todo el programa y girar solo al pintar— obliga a que el
lienzo, el histograma, el recorte y la exportación se acuerden de ella, y
alguno se olvidará.

### Lo que está probado y lo que no

Las cifras y el comportamiento de arriba salen de **cinco DNG de Expert RAW de
un Galaxy S25 Ultra**, que son los archivos reales que había. Los demás
formatos pasan exactamente por el mismo código de `rawler` —el que usa DNGLab
a diario— pero **no se han podido comprobar aquí contra archivos de verdad**.
Se ofrecen en el diálogo de abrir; si alguno falla, el fallo es de este
proyecto y no de quien lo use.

---

## Las decisiones y sus motivos

### La cáscara es la de GIMP, y lo interesante es lo que se le ha quitado

Barra de menús arriba, columna de herramientas a la izquierda, paneles que se
pliegan a la derecha, barra de estado **dentro** de la columna central —no
cruzando la ventana— para que el panel suba hasta los menús. Quien venga de
GIMP encuentra las cosas donde las deja GIMP.

Lo que NO se ha copiado, cada cosa por un motivo distinto:

- **Menús Seleccionar, Imagen, Capa y Filtros**: no hay selecciones, ni
  recorte, ni capas, ni filtros. Es el hueco de lo que no existe.
- **Menú Colores**: este es el interesante. Exposición, contraste y curva SON
  material del menú Colores de GIMP, pero allí abren un diálogo y aquí los
  tres controles están puestos para siempre en el panel. Una entrada de menú
  sería una segunda puerta al mismo deslizador.
- **Menú Herramientas**: GIMP lo necesita porque su caja se puede ocultar y
  porque tiene cuarenta herramientas. Con tres siempre visibles, es un
  duplicado.
- **Menú Ayuda**: la única entrada honrada sería «Acerca de», y ese diálogo no
  existe. Cuando exista, el menú aparece.
- **El cuadro de «Opciones de herramienta»**: mano, cuentagotas y comparar no
  tienen ni una opción. Sería una caja vacía, o sea el control que no hace
  nada, cazado antes de dibujarlo.
- **Reglas y guías**: no hay recorte ni alineación que las use, y se comerían
  40 px del único sitio donde se juzga el enfoque.
- **Acoplamiento de verdad** —arrastrar una sección a otra columna,
  desprenderla, pestañas de muelles—: centenares de líneas de geometría para
  tres paneles que siempre acaban en el mismo sitio, para permitir seis
  ordenaciones de las cuales cinco son peores que la buena. «Acoplable» es lo
  caro; «se pliega» es lo que de verdad se siente, y cuesta un `<details>`.

La regla que decide todas esas negativas es la misma: **ni un control que no
haga nada, y una sola puerta por acción.** Después de la cáscara hay
exactamente un sitio donde está cada cosa, y la tabla de atajos de abajo es la
prueba.

### El zoom no es una herramienta, y el porcentaje del pie no es un control

La lupa de GIMP no está. La rueda ya amplía anclada al cursor, el doble clic
ya ajusta a la ventana, y `Ver` tiene sus dos entradas con sus teclas `0` y
`1`: un icono de lupa sería la **cuarta** puerta al mismo acto, y la peor de
las cuatro. Quitarla no retira ninguna capacidad.

Por lo mismo, el `47,3 %` de la barra de estado es una LECTURA y no lleva
botones de [Ajustar] y [1:1] al lado. El doble clic sobre la foto ya ajusta
sin soltar el ratón, que es mejor que cualquier botón en una esquina.

### La condición para que la columna de herramientas pueda existir

En cuanto hay modos excluyentes, con el cuentagotas elegido el arrastre con el
botón izquierdo deja de desplazar la foto — y si nada lo explica, el usuario
arrastra, no pasa nada y la herramienta parece rota.

Por eso **el botón central desplaza con cualquier herramienta**, y va escrito
en el tooltip del cuentagotas, que es el momento exacto en que hace falta
saberlo. GIMP no sufre esto porque su desplazamiento no vive en la caja: vive
en el botón central, en la barra espaciadora y en las barras del lienzo, y
aquí no había ninguna de las tres. Copiar la caja sin copiar sus salidas
habría hecho la cáscara nueva peor que la barra de botones que sustituye.

### Qué dice y qué no dice el número del cuentagotas

Lee `getImageData` del lienzo ya pintado: es sRGB de 8 bits, ya revelado y ya
remuestreado al tamaño de la ventana. Dos consecuencias que hay que decir sin
rodeos:

- **No puede contestar «¿le queda detalle a esa nube?»**. Lo que pasa de 1,0
  en luz lineal llega al lienzo recortado a 255, así que ahí el número deja de
  distinguir. Esa pregunta la contesta el recuento de recorte del histograma.
- Por debajo del 100 % de zoom es la media de varios píxeles de la foto.

Por eso la casilla se llama **«En pantalla»** y no «Color»: el asterisco va en
el rótulo que el usuario lee siempre, no en un tooltip ni en un comentario del
fuente.

Y lleva un cuadradito con el color leído, que no es adorno. La primera versión
escribía solo los números, y el autor dijo que el cuentagotas «no funciona»:
funcionaba, pero escribía texto gris de 11 píxeles al fondo de la pantalla,
lejos de donde estaba mirando. **Una lectura que no se nota es una lectura que
no existe.** El color se ve con el rabillo del ojo; los números hay que ir a
leerlos. Por lo mismo, arrastrar con el cuentagotas pulsado va leyendo sobre la
marcha: recorrer la foto y ver cambiar el color es lo que lo convierte en una
herramienta en vez de en un botón que escribe un número. Una lectura honrada de la escena necesitaría un comando
`muestrear(x, y)` en Rust que leyera la imagen lineal en `f32`; no existe, y
el código lo dice donde se escribe el número.

### Seis casillas en la barra de estado, y un solo escritor por casilla

Esto arregla un fallo real, no es maquillaje. El pie era un nodo que se
vaciaba y se volvía a escribir entero, y lo escribía todo el mundo: cada
repintado del lienzo metía ahí el zoom, así que «Exportado: C:\…» —o un error
en rojo— duraba hasta que el ratón rozaba la foto. El mensaje más importante
de la sesión se perdía por mover la rueda. Con seis nodos y seis métodos que
no se pisan, eso deja de ser algo que recordar y pasa a ser imposible.

### Una tabla de acciones, que es lo único que acorta el código

El mismo dato —«Exportar como…, Ctrl+E, llama a exportar, se activa si hay
foto»— lo necesitan tres sitios: el menú lo pinta, el teclado lo dispara y el
rótulo del atajo sale de él. Antes estaba escrito tres veces, y ya había un
desajuste: un botón rotulaba «Ctrl+Y» mientras el teclado atendía además
Ctrl+Mayús+Z. Con la tabla, lo que se enseña y lo que dispara salen del mismo
sitio y no pueden separarse. El atajo se declara en CAMPOS y no en una cadena
`"Ctrl+E"`: un mini-lenguaje con su analizador es justo la infraestructura que
este proyecto evita.

### El revelado se resuelve primero y se ejecuta en una sola pasada

Lo evidente es que cada operación recorra la imagen y haga lo suyo: un bucle,
una operación, fácil de leer. Era así, y no aguantaba.

La cuenta que importa no son las multiplicaciones: es **cuántas veces se
recorre la memoria**. Una vista de 1.600 × 900 en RGB de `f32` son 17 MB, así
que cada pasada mueve 17 de lectura más 17 de escritura. Con tres operaciones
se iban unos 12 ms solo en ir y venir; con los nueve controles que vienen
serían cinco pasadas y el cuadro se saldría de los 33 ms, o sea **por debajo
de los 30 cuadros por segundo de los que presume este README**.

Ahora la lista se resuelve antes de tocar un píxel —en `pf-document/plan.rs`—
y se ejecuta en una sola pasada, estén movidos tres deslizadores o nueve. La
curva de contraste, que es la única con `powf`, se tabula; la exposición no,
porque es una multiplicación y porque tabularla malgastaría la resolución de
la tabla cuando la exposición es alta. Resultado medido: **de 28 ms a 13 por
cuadro**, con las mismas operaciones.

Esto tiene un precio y conviene reconocerlo: el código deja de leerse como
«un bucle, una operación». Lo compensan dos cosas. Cada operación conserva su
razonamiento escrito en su variante del enum, y hay una prueba que comprueba
que la pasada fusionada da exactamente lo mismo que aplicar las operaciones
una a una, sobre doscientas muestras y seis combinaciones.

Un detalle que cazó el diseño antes de que mordiera: los trozos que reparte
`rayon` ahora miden `CANALES * 1024` y no 4096. 4096 no es múltiplo de 3, así
que un trozo partía píxeles por la mitad. Hoy daba igual porque todas las
operaciones tratan cada valor por separado; la primera que mire al píxel
entero —y la siguiente lo hace, porque necesita la luminancia— pintaría
franjas de color en los bordes de cada trozo.

### Por dentro, una imagen es RGB lineal en `f32`. Siempre

No hay un tipo para 8 bits y otro para 16: abrir un JPEG consiste en
convertirlo a RGB lineal en coma flotante.

**Lineal** porque los valores de un JPEG no son luz, son luz pasada por la
curva de sRGB. Subir la exposición multiplicando esos valores no duplica la
luz: duplica un número que representa la luz de forma torcida, y el resultado
tiene los medios tonos lavados. En lineal, un paso de exposición es
literalmente multiplicar por dos, y el código de la operación cabe en tres
líneas.

**`f32`** porque un editor no destructivo encadena operaciones y con enteros
cada una redondea. Y porque la exposición puede sacar valores por encima de
1,0 que una corrección posterior devuelve al rango: con `u16` ese brillo se
habría recortado para siempre.

**Tres canales y no cuatro** porque una fotografía no tiene transparencia. Un
canal alfa que siempre vale 1 cuesta un tercio más de memoria —200 MB en una
foto de 50 Mpx— para nada.

El precio de todo esto es la memoria: una foto de 12,5 Mpx ocupa **142 MB**.
Se paga a cambio de que ninguna operación degrade la imagen.

### El contraste es la excepción, y por eso lleva su explicación encima

Se aplica en espacio perceptual y no en lineal. En luz lineal el gris medio
está en 0,18 y las sombras se amontonan por debajo de 0,05: estirar alrededor
de 0,18 movería muchísimo las luces y casi nada las sombras, así que el primer
20 % del deslizador no parecería hacer nada y el último quemaría el cielo. Un
deslizador tiene que sentirse parejo. El código está en
[`operacion.rs`](crates/pf-document/src/operacion.rs) con el razonamiento
completo.

### El orden de las operaciones es fijo, no el de uso

Si la lista se llenara por orden de uso, subir la exposición y luego el
contraste daría una foto distinta que hacerlo al revés, y el usuario no tendría
forma de saber por qué. Cada operación declara su `orden()` y la lista se
ordena sola: primero se coloca la foto donde debería haber estado (exposición)
y después se interpreta (contraste).

### Primero se recorta, después se revela

Una foto de 50 Mpx no cabe en un lienzo de 1,4 Mpx. Si cada movimiento del
deslizador recalculara los 50 millones de píxeles para enseñar 1,4 millones, se
tiraría el 97 % del trabajo.

Así que el lienzo **no tiene la foto**: tiene un zoom, un punto central y el
tamaño de la ventana. Con eso calcula qué rectángulo se ve y a qué resolución,
y se lo pide a Rust ya revelado. El coste deja de depender del tamaño de la
foto y pasa a depender del tamaño de la ventana.

Hay que decirlo en voz alta: lo que se ve en pantalla está calculado sobre
menos píxeles que el archivo final. Para exposición y contraste, que tratan
cada píxel por separado, la aproximación es exacta. Para lo que venga después
—enfoque, reducción de ruido— no lo será, y habrá que avisarlo en la interfaz.

### El puente con la interfaz manda bytes, no JSON

Un lienzo de 1.600 × 900 en RGBA son 5,7 MB. En JSON, con un número y una coma
por byte, pasarían de 20 MB de texto que construir, copiar, enviar y analizar,
treinta veces por segundo. Por eso `renderizar` devuelve
`tauri::ipc::Response`, que manda los bytes en crudo y llega al navegador como
un `ArrayBuffer` que `putImageData` usa casi sin tocarlo.

### Tres cajas, no veinte

El plan tiene veinte. Hoy hay tres:

- **`pf-core`** — el error y los tipos que todas comparten.
- **`pf-image`** — los píxeles: carga, color, muestreo, histograma,
  metadatos, exportación.
- **`pf-document`** — el documento no destructivo: el original intacto más la
  lista de operaciones.

Una caja es un sitio más donde buscar, y veinte cajas vacías son veinte sitios
donde buscar. Se parten cuando duela, no antes.

Lo que sí justifica que haya tres y no una: la aplicación de Tauri se compila
como `cdylib` y eso contagia a todo lo que viva dentro. Con el núcleo fuera,
`cargo test -p pf-image` tarda **segundos** en vez de los más de cinco minutos
que tarda la aplicación entera.

### Sin framework en la interfaz

PhotoForge tiene una pantalla. Toda su complejidad está en el lienzo —que se
pinta con `putImageData` y al que un DOM virtual no le aporta nada— y en Rust.
Un framework añadiría un paso de compilación, un modelo mental y doscientos
kilobytes para resolver un problema que no se tiene. Lo que hacía falta de
verdad era no escribir `document.createElement` cuarenta veces, y eso cabe en
[un archivo de setenta líneas](src/lib/dom.ts).

### Todo gris, y sin tema claro

No es estética. En un editor de fotografía, la interfaz es el fondo contra el
que el ojo juzga el color de la foto: un panel azulado hace que la foto parezca
cálida y un fondo claro la hace parecer oscura. Todos los grises son **neutros**
—R, G y B valen lo mismo— y el que rodea al lienzo es **#767676**, que es el
18 % de reflectancia: el gris medio fotográfico, el de una carta gris.

### El archivo original no se sobrescribe nunca

Al exportar, PhotoForge compara la ruta de destino con la del archivo abierto y
se niega si son la misma. Es el peor fallo posible de este programa y ocurre
con un solo despiste en el diálogo de guardar.

Y mientras se escribe, se escribe a un archivo `.pf-parcial` que solo se
renombra al terminar. Si el programa se cierra a mitad de un TIFF de 300 MB, en
el disco queda el archivo anterior entero en vez de uno a medias con el nombre
bueno, que es peor porque parece válido.

---

## Las cifras

Medidas con
[`cargo run -p pf-document --example revelar --release`](crates/pf-document/examples/revelar.rs)
sobre un TIFF de 16 bits de 4080 × 3060 (12,5 Mpx) salido de un DNG del S25
Ultra, con una vista de 1.600 × 900:

| Operación | Tiempo |
|---|---|
| Abrir el TIFF de 16 bits y pasarlo a RGB lineal | 122 ms |
| Muestrear la vista, sin revelar | 10 ms |
| **Vista revelada (lo que cuesta un movimiento del deslizador)** | **13 ms** |
| Pasar la vista a RGBA de 8 bits para el lienzo | 2,8 ms |
| Histograma rápido (786.000 muestras) | 19 ms |
| Histograma completo (1.040.000 muestras) | 86 ms |
| Revelar la foto entera para exportar | 50 ms |
| Escribir el JPEG de 12,5 Mpx | 266 ms |

Todas son del perfil de **publicación** (`--release`). Una compilación de
desarrollo se nota más lenta: el perfil `dev` de este proyecto ya optimiza las
cajas del núcleo (`opt-level = 3` para `pf-image` y `pf-document`), pero no la
aplicación, así que `npm run tauri dev` no es donde hay que mirar las cifras.

Los 13 ms de la vista revelada son el número que importa: dan unos **75 cuadros
por segundo** arrastrando un deslizador, y **no suben si la foto es más
grande**, porque dependen del tamaño de la ventana.

Y por eso el histograma tiene dos modos: los 86 ms del completo son
inaceptables sesenta veces por segundo, así que mientras se arrastra se pide el
rápido y el completo solo al soltar. El completo es el único que puede contar
bien el recorte; el rápido, al promediar píxeles, puede perder de vista cuatro
píxeles quemados.

La decisión de **no usar la GPU todavía** sale de estas cifras: con 13 ms por
cuadro en CPU, `wgpu` serían miles de líneas y una superficie que falla
distinto en cada equipo para arreglar algo que de momento no está roto. Se
medirá otra vez cuando haya operaciones que miren a los vecinos.

---

## Montarlo desde cero

**1. Rust 1.89 o superior**, con el toolchain de MSVC. Desde
[rustup.rs](https://rustup.rs), y después:

```bash
rustup default stable-x86_64-pc-windows-msvc
```

**2. Node.js 20.19 o superior** (o 22.12 o superior). Ese escalón lo pone
Vite 8; con un Node anterior la instalación solo suelta un aviso `EBADENGINE`
que se pierde entre el ruido y el fallo de verdad aparece mucho después.

**3. Las herramientas de C++ de Visual Studio.** En el instalador de Visual
Studio Build Tools, la carga de trabajo **«Desarrollo para el escritorio con
C++»** (*Desktop development with C++*), que trae MSVC v143 y el Windows SDK.
Aquí no se compila nada en C —ese era justamente el objetivo al elegir
`rawler`—, pero el enlazador de Rust para Windows sí es el de MSVC.

**4. WebView2 Runtime.** Viene de serie en Windows 11.

Y ya:

```bash
npm install
```

```bash
npm run tauri dev
```

La primera compilación baja y compila unas 500 cajas: cuenta con varios
minutos. `npm run tauri build` tarda siempre, porque el perfil de publicación
usa LTO completo y una sola unidad de codegen.

> **Al compilar, cierra antes la aplicación.** Si está abierta, Windows tiene
> bloqueado el `.exe`, la compilación falla al reemplazarlo **y aun así
> termina con código de salida 0**, dejando los instaladores de la vez
> anterior. Compara las fechas de los archivos para saber si la compilación es
> real.

### Las pruebas

```bash
cargo test
```

70 pruebas: 67 del núcleo, que corren en centésimas de segundo porque no
arrastran Tauri, y 3 de integración que recorren una sesión entera —abrir,
ajustar, comparar, deshacer, exportar— contra el estado real de la aplicación.

Lo que esas pruebas **no** cubren es abrir un RAW: los archivos de prueba son
fotos del autor, pesan veinte megas cada una y no están en el repositorio.
Para eso está la sonda, que se corre a mano contra tus propios archivos.

Solo el núcleo, que es lo que se toca a menudo:

```bash
cargo test -p pf-core -p pf-image -p pf-document
```

---

## Cómo está repartido

```text
crates/
  pf-core/        Error, Dimensiones, Region
  pf-image/       Imagen (RGB lineal f32)
    color.rs        sRGB <-> lineal, con sus tablas
    carga.rs        JPG, PNG, TIFF -> Imagen
    muestreo.rs     el recorte y escalado de la vista
    orientacion.rs  poner la foto derecha
    raw.rs          RAW de cámara: normalizar, revelar y exponer
    histograma.rs   256 columnas y el recuento de recorte
    metadatos.rs    EXIF
    exportar.rs     JPEG, PNG, TIFF de 16 bits
    examples/sonda_raw.rs    la prueba de riesgo del RAW
  pf-document/    Documento = original + lista de operaciones
    operacion.rs    qué hace cada operación y por qué
    plan.rs         las resuelve y las ejecuta en una sola pasada
    examples/revelar.rs      el banco de medidas

src-tauri/        la cáscara: ventana, estado y comandos
  src/comandos.rs   lo que la interfaz puede pedir
  src/estado.rs     el documento abierto
  tests/flujo.rs    una sesión entera sin ventana

src/              la interfaz
  lib/api.ts        la frontera con Rust, toda en un archivo
  lib/dom.ts        el ayudante el() y el icono() con createElementNS
  ui/menu.ts        la barra de menús, el teclado y la tabla de acciones
  ui/herramientas.ts  la columna: mano, cuentagotas y comparar
  ui/lienzo.ts      zoom, desplazamiento, pintado y cuentagotas
  ui/panel.ts       histograma, deslizadores y metadatos, plegables
  ui/estado.ts      la barra de estado, seis casillas sin pisarse
```

La regla que mantiene esto en pie: **en `src-tauri` no se escribe ni un cálculo
sobre píxeles**. Si hace falta uno, su sitio es `pf-image`.

---

## Atajos de teclado

| Tecla | Qué hace |
|---|---|
| `Ctrl+O` | Abrir |
| `Ctrl+E` | Exportar |
| `Ctrl+W` | Cerrar la foto |
| `Ctrl+Q` | Salir |
| `Ctrl+Z` / `Ctrl+Y` | Deshacer / Rehacer |
| `0` | Ajustar a la ventana |
| `1` | Tamaño real 1:1 |
| `P` | Ocultar o enseñar el panel |
| `M` / `C` | Mano / Cuentagotas |
| `\` (mantener) | Ver el original |
| Rueda | Zoom anclado al cursor |
| Arrastrar | Desplazar (con la Mano) |
| Arrastrar con el botón central | Desplazar, con cualquier herramienta |
| Doble clic | Ajustar a la ventana |

La tecla del panel es `P` y no `Tab` a propósito: `Tab` es la única cuyo
comportamiento por omisión esta interfaz necesita, porque los deslizadores son
los únicos controles a los que hoy se llega con el teclado. Robarla los
dejaría inalcanzables, que es un control muerto por la puerta de atrás.

---

## Lo que falta

En el orden en que tiene sentido hacerlo, y con el motivo del orden:

1. **Soltar una foto en la ventana para abrirla.** Hoy no existe: hace falta
   `onDragDropEvent` de Tauri, porque el `drop` del DOM se lo come el WebView.
   El sitio natural es la pantalla de «Abre una foto para empezar», y el día
   que esté, su botón «Abrir…» se retira y la aplicación llega a cero puertas
   dobles.
2. **Más operaciones de revelado**: luces, sombras, blancos, negros,
   temperatura y tinte. Cuando sean ocho o diez, el enum `Operacion` empezará a
   molestar y será el momento de partirlo; hoy no.
3. **Guardar proyectos (`.pforge`).** El formato se definirá cuando la lista de
   operaciones esté asentada. Inventarlo ahora sería diseñar a ciegas, y
   cambiarlo después rompería los archivos de quien ya lo use.
4. **Capas y máscaras.** Aquí es donde el modelo de «una lista plana de
   operaciones» se queda corto y habrá que revisarlo de verdad.
5. **GPU**, si las cifras lo piden. Hoy no lo piden.

---

## Licencia

[MIT](LICENSE).

PhotoForge usa [`rawler`](https://github.com/dnglab/dnglab) (LGPL-2.1) para
decodificar RAW. El apartado de la prueba de riesgo explica
qué implica eso y qué habría que cambiar si el proyecto dejara de ser abierto.
