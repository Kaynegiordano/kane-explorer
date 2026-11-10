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
      if (attempt >= 60 || !String(err).includes('failed to acquire webview reference')) {
        if (attempt >= 60) kaneNote('appel refusé pendant 10 s : ' + String(err).slice(0, 120));
        throw err;
      }
      kaneBoot.retries++;
      await new Promise((r) => setTimeout(r, Math.min(25 * (attempt + 1), 250)));
    }
  }
}

/** Incident noté dans demarrage.log (1.24.2) : blocage de la page, affichage qui tarde, appel refusé. */
function kaneNote(text) {
  try { window.__TAURI_INTERNALS__.invoke('diag_note', { text: String(text) }).catch(() => {}); } catch { /* moteur indisponible */ }
}

/* Page bloquée plus d'une seconde (calcul, dossier énorme…) : noté, au plus une fois toutes les 10 s. */
(() => {
  let last = 0;
  try {
    new PerformanceObserver((list) => {
      for (const e of list.getEntries()) {
        if (e.duration < 1000 || performance.now() - last < 10000) continue;
        last = performance.now();
        const where = (typeof tab !== 'undefined' && tab && tab.path) || '?';
        kaneNote(`page bloquée ${(e.duration / 1000).toFixed(1)} s (à ${(e.startTime / 1000).toFixed(1)} s, dossier ${where})`);
      }
    }).observe({ type: 'longtask', buffered: true });
  } catch { /* non pris en charge */ }
  window.addEventListener('error', (e) => { if (!kaneBoot.reported) kaneNote(`erreur au démarrage : ${e.message} @${(e.filename || '').split('/').pop()}:${e.lineno}`); });
})();

/** Fin du démarrage (contenu affiché) : envoie les temps au moteur, une seule fois par page. */
function reportStartup() {
  if (kaneBoot.reported) return;
  kaneBoot.reported = true;
  // La page est prête mais rien n'est encore dessiné au bout de 3 s : fenêtre figée côté affichage, noté
  let painted = false;
  setTimeout(() => { if (!painted) kaneNote('page prête mais pas encore affichée après 3 s'); }, 3000);
  requestAnimationFrame(() => {
    painted = true;
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
