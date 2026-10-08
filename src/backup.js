'use strict';
/* Sauvegarde et restauration des réglages : épinglés, accès rapide, recherches enregistrées, sessions, colonnes,
   notes en étoiles, étiquettes, apparence, options... (tout `kane.*` du localStorage, sauf ce qui est propre à une
   session : onglets ouverts, étagère, historique récent, fenêtres ouvertes). Chargé en dernier. */

const BACKUP_SKIP = /^kane\.(tabs|win\..*|lastOrganize|justUpdated|updateNotified|recent|shelf)$/;
const BACKUP_NAME = /^Kane-reglages.*\.json$/i;

function settingsSnapshot() {
  const data = {};
  for (let i = 0; i < localStorage.length; i++) {
    const k = localStorage.key(i);
    if (k.startsWith('kane.') && !BACKUP_SKIP.test(k)) data[k] = localStorage.getItem(k);
  }
  return data;
}

const documentsDir = () => shared.places.find((p) => p.kind === 'documents')?.path;

/** Écrit les réglages dans « Documents\Kane-reglages-AAAA-MM-JJ.json » (jamais écrasé : un numéro s'ajoute si besoin). */
async function exportSettings() {
  const dir = documentsDir();
  if (!dir) { toast('Dossier Documents introuvable', 'error'); return; }
  const data = settingsSnapshot();
  const version = await window.__TAURI__.app.getVersion().catch(() => '');
  const json = JSON.stringify({ app: 'Kane Explorer', version, date: new Date().toISOString(), data }, null, 2);
  const name = `Kane-reglages-${new Date().toISOString().slice(0, 10)}.json`;
  try {
    const path = await invoke('create_file', { parent: dir, name, content: json });
    toast(`${plural(Object.keys(data).length, 'réglage')} enregistrés dans « ${basename(path)} »`);
    winCall('reveal_path', { path });
  } catch (err) { toast(cleanError(err), 'error'); }
}

/** Remplace les réglages par ceux d'un fichier de sauvegarde, puis recharge Kane. */
async function restoreSettings(path) {
  let obj;
  try {
    const text = await invoke('read_text', { path, max: 8_000_000 });
    obj = JSON.parse(text);
  } catch { toast('Fichier de réglages illisible', 'error'); return; }
  const data = obj?.app === 'Kane Explorer' && obj.data && typeof obj.data === 'object' ? obj.data : null;
  const keys = data ? Object.keys(data).filter((k) => k.startsWith('kane.') && !BACKUP_SKIP.test(k) && typeof data[k] === 'string') : [];
  if (!keys.length) { toast('Ce fichier n’est pas une sauvegarde de réglages Kane Explorer', 'error'); return; }
  if (!(await confirmDialog(`Restaurer ${plural(keys.length, 'réglage')} depuis « ${basename(path)} » ? Les réglages actuels du même nom seront remplacés et Kane va se recharger.`, 'Restaurer', false))) return;
  for (const k of keys) { try { localStorage.setItem(k, data[k]); } catch { /* ignoré */ } }
  location.reload();
}

async function importSettings() {
  const dir = documentsDir();
  let files = [];
  if (dir) {
    try { files = (await invoke('list_dir', { path: dir })).filter((e) => !e.is_dir && BACKUP_NAME.test(e.name)).sort((a, b) => b.modified - a.modified).slice(0, 8); } catch { /* dossier illisible */ }
  }
  openModal(
    `<h2>Restaurer les réglages</h2>` +
    (files.length
      ? `<p class="lead">Sauvegardes trouvées dans Documents :</p><div class="list-box">${files.map((f, i) => `<button class="btn" data-f="${i}" style="display:block;width:100%;text-align:left;margin-bottom:6px">${esc(f.name)} <small style="color:var(--muted)">${fmtDate(f.modified)}</small></button>`).join('')}</div>`
      : `<p class="lead">Aucune sauvegarde « Kane-reglages-….json » dans Documents.</p>`) +
    `<div class="foot"><button class="btn" data-f="other">Autre fichier…</button><button class="btn primary" data-f="close">Fermer</button></div>`,
  );
  modalBox.onclick = async (ev) => {
    const b = ev.target.closest('[data-f]');
    if (!b) return;
    const f = b.dataset.f;
    closeModal();
    if (f === 'close') return;
    if (f === 'other') {
      const p = await promptDialog('Chemin du fichier de réglages', dir ? `${dir}\\Kane-reglages.json` : '');
      if (p) restoreSettings(p.trim().replace(/^"|"$/g, ''));
    } else restoreSettings(files[+f].path);
  };
}
