# Cambios

Formato: [Keep a Changelog](https://keepachangelog.com/es-ES/1.1.0/). Las notas de cada versión
se publican tal cual en su *release* de GitHub (el workflow toma la sección de la versión).

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
