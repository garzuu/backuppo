(function () {
  // Risolto rispetto all'URL di questo script, non della pagina che lo
  // include: così funziona identico dalla landing IT (root) e da quella
  // EN (/en/), senza hardcodare il base path del sito Astro.
  var scriptUrl = document.currentScript && document.currentScript.src;
  var castUrl = scriptUrl ? new URL("backuppo-demo.cast", scriptUrl).toString() : "backuppo-demo.cast";

  window.addEventListener("DOMContentLoaded", () => {
    const target = document.getElementById("backuppo-demo-player");
    if (target && window.AsciinemaPlayer) {
      window.AsciinemaPlayer.create(castUrl, target, {
        autoPlay: true,
        loop: true,
        theme: "monokai",
        fit: "width",
      });
    }
  });
})();
