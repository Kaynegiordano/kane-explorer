'use strict';
/* Règles automatiques sur un dossier (1.23.0, rules.rs) : « quand un fichier arrive ici : le débloquer, en faire une
   copie JPG, le ranger dans un sous-dossier ». Fenêtre de réglage (clic droit sur le vide → Règles automatiques),
   pastille dans la barre d'état, message quand une règle a agi. Chargé après tools.js. */

const RULE_WHAT = [
  ['image', 'Images'], ['heic', 'Photos d’iPhone (HEIC)'], ['video', 'Vidéos'], ['audio', 'Sons'], ['archive', 'Archives (zip, 7z, rar…)'],
  ['doc', 'Documents (PDF, Word, Excel…)'], ['app', 'Programmes (exe, msi)'], ['all', 'Tous les fichiers'], ['ext', 'Extensions…'],
];
const RULE_ACTIONS = [['move', 'les ranger dans le sous-dossier'], ['jpg', 'en faire une copie JPG à côté'], ['unblock', 'les débloquer (marque « téléchargé »)']];
const RULE_PRESETS = [
  { what: 'heic', action: 'jpg', dest: '', label: 'Photos d’iPhone → copie JPG' },
  { what: 'archive', action: 'move', dest: 'Archives', label: 'Archives → « Archives »' },
  { what: 'app', action: 'move', dest: 'Programmes', label: 'Programmes → « Programmes »' },
  { what: 'image', action: 'move', dest: 'Images', label: 'Images → « Images »' },
  { what: 'video', action: 'move', dest: 'Vidéos', label: 'Vidéos → « Vidéos »' },
  { what: 'all', action: 'unblock', dest: '', label: 'Tout débloquer' },
];

let kRules = [];
const rulesHere = (path = tab?.path) => kRules.filter((r) => path && samePath(r.folder, path));

async function loadRules() {
  kRules = await invoke('rules_list').catch(() => kRules);
  renderRulesBadge();
}

async function saveRules() {
  try { await invoke('rules_save', { rules: kRules }); } catch (e) { toast(cleanError(e), 'error'); }
  renderRulesBadge();
}

/** Pastille « ⚡ 2 règles » dans la barre d'état du dossier qui a des règles actives. */
function renderRulesBadge() {
  let b = $('status-rules');
  if (!b) {
    b = Object.assign(document.createElement('button'), { id: 'status-rules', className: 'status-rules', title: 'Règles automatiques de ce dossier' });
    b.onclick = () => openRules();
    $('status-extra')?.before(b);
  }
  const n = tab && tab.path !== HOME && !tab.virtual && !tab.trash ? rulesHere().filter((r) => r.on).length : 0;
  b.hidden = !n;
  b.textContent = n ? `⚡ ${plural(n, 'règle')}` : '';
}

function ruleLine(r) {
  const what = r.what.startsWith('ext:') ? 'ext' : r.what;
  return `<div class="rule-row" data-id="${r.id}">
    <label class="rule-on" title="Activer / désactiver"><input type="checkbox" data-k="on" ${r.on ? 'checked' : ''}></label>
    <span>Quand arrivent</span>
    <select data-k="what">${RULE_WHAT.map(([v, l]) => `<option value="${v}" ${v === what ? 'selected' : ''}>${l}</option>`).join('')}</select>
    <input type="text" data-k="exts" placeholder="png, jpg" value="${esc(r.what.startsWith('ext:') ? r.what.slice(4) : '')}" ${what === 'ext' ? '' : 'hidden'} spellcheck="false">
    <select data-k="action">${RULE_ACTIONS.map(([v, l]) => `<option value="${v}" ${v === r.action ? 'selected' : ''}>${l}</option>`).join('')}</select>
    <input type="text" data-k="dest" placeholder="Nom du sous-dossier" value="${esc(r.dest)}" ${r.action === 'move' ? '' : 'hidden'} spellcheck="false">
    <button class="btn" data-x="del" title="Supprimer cette règle">✕</button></div>`;
}

async function openRules(folder = tab.path) {
  if (!folder || folder === HOME || tab.virtual || tab.trash) return;
  await loadRules();
  const draw = () => {
    const list = rulesHere(folder);
    modalBox.querySelector('.rule-list').innerHTML = list.length ? list.map(ruleLine).join('')
      : '<p class="lead">Aucune règle pour ce dossier. Choisissez un modèle ci-dessous, ou « Ajouter une règle ».</p>';
  };
  openWideModal(`<h2>Règles automatiques</h2>
    <p class="lead">Dans « ${esc(basename(folder) || folder)} », Kane agit tout seul sur chaque fichier qui arrive (téléchargement, copie…),
      une fois le fichier complet. Rien n’est supprimé ni écrasé. Fonctionne tant que Kane est ouvert, ou lancé avec Windows
      (Options → « Garder Kane prêt en arrière-plan »).</p>
    <div class="rule-list"></div>
    <div class="rule-presets"><span>Modèles :</span>${RULE_PRESETS.map((p, i) => `<button class="btn" data-preset="${i}">${esc(p.label)}</button>`).join('')}</div>
    <div class="rule-log"></div>
    <div class="foot"><button class="btn" data-x="add">Ajouter une règle</button>
      <div style="display:flex;gap:8px"><button class="btn" data-x="now">Appliquer aux fichiers déjà là</button><button class="btn primary" data-x="close">Fermer</button></div></div>`);
  draw();
  const showLog = async () => {
    const lines = (await invoke('rules_log').catch(() => [])).filter((l) => true).slice(0, 8);
    const el = modalBox.querySelector('.rule-log');
    if (el) el.innerHTML = lines.length ? `<b>Dernières actions</b>${lines.map((l) => { const [t, x] = l.split('\t'); return `<div><span>${dateFmt.format(+t * 1000)}</span> ${esc(x)}</div>`; }).join('')}` : '';
  };
  showLog();
  const add = (r) => { kRules.push({ id: Date.now() + Math.floor(Math.random() * 1000), folder, on: true, what: 'image', action: 'move', dest: 'Images', ...r }); saveRules(); draw(); };
  modalBox.onclick = async (ev) => {
    const x = ev.target.closest('[data-x]')?.dataset.x;
    const preset = ev.target.closest('[data-preset]')?.dataset.preset;
    if (preset !== undefined) { const p = RULE_PRESETS[+preset]; add({ what: p.what, action: p.action, dest: p.dest }); return; }
    if (x === 'close') closeModal();
    else if (x === 'add') add({});
    else if (x === 'del') {
      const id = +ev.target.closest('.rule-row').dataset.id;
      kRules = kRules.filter((r) => r.id !== id);
      saveRules(); draw();
    } else if (x === 'now') {
      const done = await invoke('rules_apply_now', { folder }).catch((e) => { toast(cleanError(e), 'error'); return null; });
      if (!done) return;
      toast(done.length ? `${plural(done.length, 'action')} : ${done.slice(0, 2).join(' · ')}${done.length > 2 ? '…' : ''}` : 'Rien à faire : aucun fichier concerné');
      showLog();
      if (samePath(tab.path, folder)) refresh(true);
    }
  };
  modalBox.onchange = (ev) => {
    const row = ev.target.closest('.rule-row');
    if (!row) return;
    const r = kRules.find((x) => x.id === +row.dataset.id);
    const k = ev.target.dataset.k;
    if (!r || !k) return;
    if (k === 'on') r.on = ev.target.checked;
    else if (k === 'what') r.what = ev.target.value === 'ext' ? 'ext:' + (row.querySelector('[data-k=exts]').value || '') : ev.target.value;
    else if (k === 'exts') r.what = 'ext:' + ev.target.value;
    else if (k === 'action') { r.action = ev.target.value; if (r.action === 'move' && !r.dest) r.dest = 'Rangés'; }
    else if (k === 'dest') r.dest = ev.target.value.trim().replace(/[:*?"<>|]/g, '');
    saveRules();
    row.outerHTML = ruleLine(r);
  };
}

// Une règle a agi (dans n'importe quel dossier surveillé) : message, et le dossier affiché se met à jour seul (surveillance)
let rulesToastAt = 0;
listen('rules-applied', ({ payload }) => {
  const { folder, text } = payload || {};
  if (!text || Date.now() - rulesToastAt < 400) return; // rafale (dossier entier) : un message suffit
  rulesToastAt = Date.now();
  toast(`⚡ ${basename(folder) || folder} : ${text}`);
});

window.addEventListener('DOMContentLoaded', () => setTimeout(loadRules, 1200));
