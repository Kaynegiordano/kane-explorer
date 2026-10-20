'use strict';

/* Chargé avant tous les autres scripts.

   1) Appels refusés pendant la naissance d'une fenêtre. Tauri n'enregistre une fenêtre qu'après l'avoir créée ; une page
      qui démarre très vite (fenêtre ouverte depuis un autre fil : Windows + E, nouvelle fenêtre, Explorateur repris) peut
      appeler le moteur juste avant. Tauri répond alors « failed to acquire webview reference » : le dossier ne se chargeait
      pas, les évènements n'étaient pas écoutés et des messages d'erreur s'empilaient. Ces appels sont simplement retentés
      quelques millisecondes plus tard (tous les appels passent par __TAURI_INTERNALS__.invoke, y compris ceux de Tauri).

   2) Journal de démarrage : temps de chaque étape, écrit par le moteur (demarrage.log, menu du logo → Dépannage). */

const kaneBoot = { retries: 0, reported: false };

(() => {
  const internals = window.__TAURI_INTERNALS__;
  if (!internals?.invoke) return;
  const original = internals.invoke.bind(internals);
  const NOT_READY = 'failed to acquire webview reference';
  const MAX_TRIES = 60; // ~10 s au plus, puis l'erreur remonte normalement
  internals.invoke = async (cmd, args, options) => {
    for (let attempt = 0; ; attempt++) {
      try {
        return await original(cmd, args, options);
      } catch (err) {
        if (attempt >= MAX_TRIES || !String(err).includes(NOT_READY)) throw err;
        kaneBoot.retries++;
        await new Promise((r) => setTimeout(r, Math.min(25 * (attempt + 1), 250)));
      }
    }
  };
})();

/** Fin du démarrage (contenu affiché) : envoie les temps au moteur, une seule fois par page. */
function reportStartup() {
  if (kaneBoot.reported) return;
  kaneBoot.reported = true;
  requestAnimationFrame(() => {
    const origin = performance.timeOrigin;
    const nav = performance.getEntriesByType('navigation')[0];
    window.__TAURI_INTERNALS__.invoke('startup_report', {
      pageStart: Math.round(origin),
      domReady: Math.round(origin + (nav?.domContentLoadedEventEnd || 0)),
      content: Math.round(origin + performance.now()),
      retries: kaneBoot.retries,
      when: new Date().toLocaleString('fr-FR'),
    }).catch(() => {});
  });
}
