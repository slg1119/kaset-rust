const { invoke } = window.__TAURI__.core;
const log = (m) => { document.querySelector('#log').textContent += m + '\n'; };
document.querySelector('#login').onclick = () => invoke('open_login').catch((e) => log('ERR ' + e));
document.querySelector('#dump').onclick = async () => {
  try { log(JSON.stringify(await invoke('dump_cookies'), null, 1)); } catch (e) { log('ERR ' + e); }
};
for (const [id, visible] of [['play-visible', true], ['play-hidden', false]]) {
  document.querySelector('#' + id).onclick = () =>
    invoke('open_player', { videoId: document.querySelector('#vid').value.trim(), visible })
      .catch((e) => log('ERR ' + e));
}
