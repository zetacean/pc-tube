# PC Tube

Envoltorio de YouTube para Windows, escrito en Rust, **sin Electron**: usa el WebView2 de
Windows (el motor de Edge) y le inyecta scripts, al estilo de
[pear-desktop](https://github.com/pear-devs/pear-desktop) y
[TizenTube](https://github.com/reisxd/TizenTube).

Carga la interfaz normal de <https://www.youtube.com>.

## Qué incluye

- **Bloqueo de anuncios** (vídeo y feeds) y **SponsorBlock** con modo por categoría
  (saltar solo, botón para saltar, solo marcar, desactivado).
- **DeArrow**: títulos y miniaturas de la comunidad, y corrección de títulos EN MAYÚSCULAS.
- **Apariencia** a golpe de clic (temas, color de acento, zoom, miniaturas, qué mostrar), filtros del feed
  (canales y palabras) y aviso de nuevas versiones (Ajustes → Apariencia, Interfaz y Acerca de).
- **Capítulos y marcadores**: salta entre capítulos (los de la descripción del vídeo) y guarda tus
  propios marcadores con nombre; panel con `Alt+C`.
- **Subtítulos preferidos**: se activan solos en tu idioma (o se desactivan), con traducción opcional.
- **Aviso de «ahora suena»**: notificación de Windows con la miniatura cuando empieza un vídeo y la
  ventana no está a la vista.
- **Cola de descargas**: varias a la vez en cola, cancelar, reintentar las fallidas y retomar lo que
  quedó pendiente al cerrar la app.
- **Actualización dentro de la app** (copia instalada): descarga el instalador de GitHub, comprueba su
  suma SHA-256 y se actualiza sola.
- **Mini-reproductor** (botón en el reproductor o `Alt+M`): ventana pequeña siempre visible en la
  esquina. Al cerrarlo puedes volver a la ventana completa o pausar el vídeo (configurable).
- **Descargas** de vídeos y listas de reproducción completas con yt-dlp (`Alt+D`): diálogo con las resoluciones reales del vídeo, formato
  (MP4/MKV/WebM) o solo audio (MP3/M4A/Opus/FLAC/WAV), progreso, cancelar, y lista de
  archivos descargados. Instala yt-dlp y ffmpeg desde sus repositorios oficiales,
  verificados con SHA-256.
- **Bandeja del sistema** con reproducir/pausar, siguiente y cerrar; y **botones en la
  miniatura de la barra de tareas** (anterior, reproducir/pausar, siguiente).
- Ventana sin bordes con barra propia, botón atrás/recargar, pantalla de carga, recuerda
  tamaño y posición, «Iniciar con Windows», «Mantener siempre visible».
- **Reproducción**: calidad preferida, códec (evitar AV1 baja el consumo de CPU) y
  volumen por encima del 100 %.
- Ocultar Shorts, tarjetas del final, vídeos ya vistos, y más. Todo se configura en el
  panel de ajustes (`Ctrl+,` o el engranaje de la barra superior), con exportar/importar.

## Requisitos

- Windows 10 u 11 con el [runtime de WebView2](https://developer.microsoft.com/microsoft-edge/webview2/)
  (viene con Windows 11 y con Microsoft Edge).
- Para compilar: Rust (toolchain MSVC) y las herramientas de compilación de Visual Studio.

## Instalar

Descarga de la página de Releases del repositorio el instalador `PC-Tube-<versión>-setup.exe`
(no pide permisos de administrador) o el zip portátil. Para publicar una versión: sube la
versión en `Cargo.toml`, añade su sección en `CHANGELOG.md`, y crea y sube la etiqueta
(`git tag v0.2.0 && git push --tags`); el workflow `release.yml` ejecuta los tests, compila,
genera el zip y el instalador y publica el *release* con esas notas.

## Compilar y ejecutar

```powershell
cargo run                 # versión de desarrollo
cargo build --release     # versión final: target\release\pc_tube.exe
.\tools\package.ps1       # compila en release y crea dist\PC-Tube-<versión>-win64.zip
```

## Dónde guarda las cosas

`%LOCALAPPDATA%\PcTube\`: `config.json` (ajustes), `window.json` (tamaño y posición),
`webview\` (tu sesión de YouTube) y `bin\` (yt-dlp y ffmpeg). Las descargas van a
`Descargas\PC Tube`.

## Atajos

Todos se ven y se pueden cambiar en **Ajustes → Atajos** (se avisa si dos chocan). Estos son los
de fábrica:

| Atajo | Acción |
|---|---|
| `Ctrl + ,` | Abrir los ajustes |
| `Alt + M` | Mini-reproductor |
| `Alt + D` | Descargar el vídeo actual (o la lista) |
| `Alt + ↑` / `Alt + ↓` | Velocidad más rápida / más lenta (hasta ×4) |
| `Alt + L` | Repetir el vídeo |
| `Alt + K` | Repetir un tramo (inicio, final, quitar) |
| `Alt + C` | Panel de capítulos y marcadores |
| `Alt + .` / `Alt + ,` | Capítulo siguiente / anterior |
| `Alt + B` | Añadir un marcador en este punto |
| `Ctrl + Alt + Espacio` / `→` / `←` | Reproducir-pausar / siguiente / anterior, desde cualquier aplicación |

## Desarrollo

Las compilaciones de depuración aceptan `PCTUBE_DEBUG_PORT=9222` para inspeccionar la página
con cualquier cliente CDP (`http://127.0.0.1:9222/json`). Esa sesión usa su propio perfil,
`config-debug.json` y `window-debug.json`, y no comprueba actualizaciones. En cualquier
compilación, `PCTUBE_NO_AUTOUPDATE=1` desactiva la actualización semanal de yt-dlp.

Todo lo que depende de cómo YouTube construye su página (anuncios, Shorts, DeArrow, botones
del reproductor) puede romperse con un cambio suyo; los selectores están en `scripts/`.

## Licencia

[GPL-3.0](LICENSE). Partes de adblock, SponsorBlock y DeArrow están adaptadas de
[TizenTube](https://github.com/reisxd/TizenTube) (GPL-3.0). La idea de la inyección de scripts
viene de pear-desktop (MIT).

No está afiliado a Google ni a YouTube. Descargar vídeos de YouTube puede ir contra sus
condiciones de uso: es una opción que se puede desactivar y su uso es responsabilidad de quien la use.
