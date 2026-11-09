'use strict';

/* Chargé avant tous les autres scripts.

   1) Appels refusés pendant la naissance d'une fenêtre. Tauri n'enregistre une fenêtre qu'après l'avoir créée ; une page
      qui démarre très vite (fenêtre ouverte depuis un autre fil : Windows + E, nouvelle fenêtre, Explorateur repris) peut
      appeler le moteur juste avant. Tauri répond alors « failed to acquire webview reference » : le dossier ne se chargeait
      pas, les évènements n'étaient pas écoutés et des messages d'erreur s'empilaient. Ces appels sont simplement retentés
      quelques millisecondes plus tard (tous les appels passent par __TAURI_INTERNALS__.invoke, y compris ceux de Tauri).

   2) Journal de démarrage : temps de chaque étape, écrit par le moteur (demarrage.log, menu du logo → Dépannage). */

const kaneBoot = { retries: 0, reported: false };

/** Retente un appel refusé pendant la naissance de la fenêtre (~10 s au plus, puis l'erreur remonte normalement).
    Jusqu'en 1.22.0, boot.js remplaçait `__TAURI_INTERNALS__.invoke` ; Tauri 2 l'a figé (propriété en lecture seule) :
    l'affectation levait une erreur qui arrêtait tout ce fichier, et plus rien n'était retenté. Les appels de Kane
    passent donc par `kaneRetry` (fonctions `invoke` et `listen` de main.js). */
async function kaneRetry(call) {
  for (let attempt = 0; ; attempt++) {
    try {
      return await call();
    } catch (err) {
      if (attempt >= 60 || !String(err).includes('failed to acquire webview reference')) throw err;
      kaneBoot.retries++;
      await new Promise((r) => setTimeout(r, Math.min(25 * (attempt + 1), 250)));
    }
  }
}

/** Fin du démarrage (contenu affiché) : envoie les temps au moteur, une seule fois par page. */
function reportStartup() {
  if (kaneBoot.reported) return;
  kaneBoot.reported = true;
  requestAnimationFrame(() => {
    const origin = performance.timeOrigin;
    const nav = performance.getEntriesByType('navigation')[0];
    kaneRetry(() => window.__TAURI_INTERNALS__.invoke('startup_report', {
      pageStart: Math.round(origin),
      domReady: Math.round(origin + (nav?.domContentLoadedEventEnd || 0)),
      content: Math.round(origin + performance.now()),
      retries: kaneBoot.retries,
      when: new Date().toLocaleString('fr-FR'),
    })).catch(() => {});
  });
}

/* 3) Double panneau (1.23.0, dual.js) : chaque écouteur posé sur la zone des fichiers (#content) est noté, pour être
   reposé à l'identique sur le second panneau ; un panneau ne réagit que s'il est le panneau actif (kanePaneGuard). */
const kaneContentListeners = [];
(() => {
  const c = document.getElementById('content');
  if (!c) return;
  const add = c.addEventListener.bind(c);
  c.addEventListener = (type, fn, opts) => {
    kaneContentListeners.push([type, fn, opts]);
    add(type, (ev) => { if (window.kanePaneGuard && !window.kanePaneGuard(c)) return; fn.call(c, ev); }, opts);
  };
})();
