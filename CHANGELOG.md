# Cambios

Formato: [Keep a Changelog](https://keepachangelog.com/es-ES/1.1.0/). Las notas de cada versión
se publican tal cual en su *release* de GitHub (el workflow toma la sección de la versión).

## [0.4.0]

### Añadido
- **Atajos configurables** (Ajustes → Atajos): cada atajo se cambia pulsando la combinación nueva,
  con aviso si choca con otro; los atajos globales (Ctrl+Alt+Espacio, →, ←) también, y se avisa
  si otra aplicación ya usa la combinación.
- **Subtítulos preferidos** (Ajustes → Reproducción): activarlos solos en tu idioma, aceptar los
  automáticos, traducir si no hay en tu idioma, o desactivarlos siempre.
- **Capítulos y marcadores**: capítulos a partir de las marcas de tiempo de la descripción, salto al
  capítulo siguiente o anterior, y marcadores propios por vídeo con nombre, que se pueden copiar como
  lista con enlaces. Panel con `Alt+C` o con el botón nuevo del reproductor.
- **Aviso de «ahora suena»**: notificación de Windows con la miniatura al empezar un vídeo (por
  defecto solo si la ventana no está a la vista; también «siempre» o «nunca»).
- **Cola de descargas**: las descargas nuevas se añaden a la cola y siguen solas; se pueden quitar de
  la cola, vaciarla, reintentar las que fallaron, y lo que quedó pendiente al cerrar la app se puede
  reanudar al abrirla.
- **Actualización dentro de la app**: «Actualizar ahora» en Ajustes → Acerca de descarga el
  instalador nuevo, verifica su suma SHA-256 con la publicada en el release, lo instala en silencio y
  reabre la app. Solo en la copia instalada; la portátil abre la página de descargas.

### Cambiado
- La velocidad usa ahora `Alt+↑` y `Alt+↓` (antes `Alt+]` y `Alt+[`, que en teclado español exigen
  AltGr). Todos los atajos se pueden cambiar en Ajustes → Atajos.

## [0.3.0]

### Añadido
- **Aviso de nueva versión**: al abrir la app consulta la última versión publicada en GitHub y
  avisa una vez por versión. En Ajustes → Acerca de hay un botón para buscar ahora y abrir la
  página de la versión. Solo lee; no instala nada. Se puede desactivar.
- **Filtros del feed**: oculta vídeos por canal (nombre o @usuario) o por palabras del título, en
  Inicio, Suscripciones, Búsqueda y Relacionados (Ajustes → Interfaz).
- **Apariencia** (Ajustes → Apariencia), sin escribir código: modo claro u oscuro (con un botón de sol/luna en la barra superior), 7 temas oscuros y 6 claros con vista previa y uno propio por modo con selector de color, color de acento, tamaño de la página, miniaturas cuadradas o redondeadas, miniaturas por fila, y ocultar la barra de categorías, los vídeos sugeridos, los comentarios, el chat en directo y la publicidad de Premium. Incluye «Restablecer apariencia».
- **Editor de CSS** para quien sepa, oculto dentro de Apariencia hasta que se pide.

## [0.2.0]

### Añadido
- **Mini-reproductor** (`Alt+M` o botón del reproductor): ventana pequeña siempre visible en la
  esquina, con el reproductor completo. Al cerrarlo (✕) puedes volver a la ventana completa o
  pausar el vídeo y cerrar (Ajustes → Varios).
- **Descarga de listas de reproducción** completas, cada vídeo con el formato elegido y en una
  carpeta propia. Los vídeos privados o eliminados se saltan.
- **Atajos globales**: `Ctrl+Alt+Espacio` reproducir/pausar, `Ctrl+Alt+→` siguiente,
  `Ctrl+Alt+←` anterior, desde cualquier aplicación y con la ventana en la bandeja.
- **Velocidad hasta ×4** (`Alt+]` / `Alt+[`), opción de recordarla entre vídeos, repetir el
  vídeo (`Alt+L`) y repetir un tramo (`Alt+K`).
- **Instalador** (`PC-Tube-<versión>-setup.exe`, sin permisos de administrador) además del zip
  portátil.

### Cambiado
- Se elimina la imagen en imagen nativa: el mini-reproductor la sustituye.

## [0.1.0]

Primera versión: bloqueo de anuncios, SponsorBlock, DeArrow, panel de ajustes, descargas con
yt-dlp, bandeja del sistema, botones en la miniatura de la barra de tareas, iniciar con Windows,
calidad y códec preferidos, amplificador de volumen, exportar/importar ajustes.
