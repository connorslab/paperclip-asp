'use strict';
let token = '', submitted = false;
const $ = id => document.getElementById(id);
async function api(path, body) {
  const r = await fetch('/api/' + path, {method: body ? 'POST' : 'GET', redirect: 'error', cache: 'no-store',
    headers: {Authorization: 'Bearer ' + token, 'Content-Type': 'application/json'},
    ...(body ? {body: JSON.stringify(body)} : {})});
  if (!r.ok) throw Error('Request failed (' + r.status + '). Refresh status before retrying.');
  return r.json();
}
async function refresh() {
  const state = await api('status');
  $('details').textContent = JSON.stringify(state, null, 2);
  for (const [id, healthy] of [['asp-health', state.processes.asp], ['watchman-health', state.processes.watchman], ['rpc-health', state.asp_rpc_ready]]) {
    $(id).textContent = healthy ? (id === 'rpc-health' ? 'Connected' : 'Running') : 'Not ready';
    $(id).className = healthy ? 'healthy' : 'waiting';
  }
  $('first-install').hidden = state.initialized || state.initializing || state.initialization_failed;
  const healthy = state.processes.asp && state.processes.watchman && state.asp_rpc_ready;
  $('next-title').textContent = state.initialization_failed ? 'Initialization needs attention' : state.initializing ? 'Your server is starting' : !state.initialized ? 'Set up your server' : healthy ? 'Ready for private testing' : 'Check your services';
  $('next-copy').textContent = state.initialization_failed ? 'Review the private server logs before you retry. Do not replace existing keys.' : state.initializing ? 'Refresh status shortly. Do not initialize again.' : !state.initialized ? 'Create a new server below, or restore a complete backup.' : healthy ? (state.lightning_enabled ? 'Local services respond. Lightning is configured; verify channel and Ark pool liquidity before testing payments.' : 'Local services respond. Lightning is disabled.') : 'A service is not ready. Refresh status, then review the private logs if it does not recover.';
  $('updated').textContent = 'Last checked ' + new Date().toLocaleTimeString([], {hour: '2-digit', minute: '2-digit'});
  $('initialize').disabled = submitted || state.initialized || state.initializing || state.initialization_failed;
}
async function run(operation) {
  const buttons = [$('unlock').querySelector('button'), $('refresh')];
  buttons.forEach(button => { button.disabled = true; });
  try { await operation(); } catch (e) { $('status').textContent = e.message; }
  finally { buttons.forEach(button => { button.disabled = false; }); }
}
$('unlock').onsubmit = e => { e.preventDefault(); run(async () => {
  token = $('password').value; $('password').value = '';
  try { await refresh(); } catch (e) { token = ''; throw e; }
  document.body.classList.add('session-open'); $('login').hidden = true; $('panel').hidden = false; $('status').textContent = 'Operator session unlocked.';
}); };
$('refresh').onclick = () => run(refresh);
$('lock').onclick = () => { document.body.classList.remove('session-open'); $('status').textContent = 'Session locked.'; token = ''; $('details').textContent = ''; $('panel').hidden = true; $('login').hidden = false; };
$('initialize').onclick = () => run(async () => {
  if (submitted || !confirm('Create NEW ASP keys and database state? Cancel if you need to restore an existing ASP.')) return;
  submitted = true; $('initialize').disabled = true;
  await api('initialize', {confirm: 'CREATE NEW ASP'});
  $('status').textContent = 'Initialization started. Update status shortly. Do not fund yet.';
  await refresh();
});
