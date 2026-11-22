// Kaset player bridge. Runs in the hidden player window before page scripts,
// in the main frame only. Reports the <video> element's state to Rust through
// the `playback_event` command, the only command this origin may call, and
// exposes play/pause/seek for Rust to call with `eval`.
(() => {
  'use strict';
  if (window.top !== window || location.origin !== 'https://music.youtube.com') return;
  if (window.__kasetPlayer) return;

  const VIDEO_ID = /^[A-Za-z0-9_-]{11}$/;
  const MAX_AUTOPLAY_ATTEMPTS = 5;
  const AUTOPLAY_RETRY_MS = 500;
  const PROGRESS_INTERVAL_MS = 250;

  // Rust navigates to watch?v=<id> for every song, so the URL names the song
  // this document was loaded for. The page's own autoplay changes it, and Rust
  // ignores events for any other id.
  function currentVideoId() {
    try {
      const id = new URL(location.href).searchParams.get('v') || '';
      return VIDEO_ID.test(id) ? id : null;
    } catch (_) {
      return null;
    }
  }

  function send(event) {
    const internals = window.__TAURI_INTERNALS__;
    if (!internals || typeof internals.invoke !== 'function') return;
    Promise.resolve(internals.invoke('playback_event', { event })).catch(() => {});
  }

  // Minimal ad detection, after PlaybackAdDetectionScript in the Swift app.
  function isAd() {
    const moviePlayer = document.getElementById('movie_player');
    if (moviePlayer && moviePlayer.classList
      && (moviePlayer.classList.contains('ad-showing')
        || moviePlayer.classList.contains('ad-interrupting'))) {
      return true;
    }
    const musicPlayer = document.querySelector('ytmusic-player');
    for (const api of [moviePlayer, musicPlayer && musicPlayer.playerApi]) {
      try {
        // Player type 2 is ad media.
        if (api && typeof api.getPresentingPlayerType === 'function'
          && api.getPresentingPlayerType(true) === 2) {
          return true;
        }
      } catch (_) {}
    }
    return false;
  }

  function seconds(value) {
    return Number.isFinite(value) && value >= 0 ? value : 0;
  }

  // `hold` blocks playback after a pause command or after the song ended, so
  // the page's own autoplay cannot start a song Kaset did not ask for.
  let hold = false;
  // A fresh document starts its song; YouTube Music's page does not.
  let autoplayPending = true;
  let autoplayAttempts = 0;
  let hookedVideo = null;
  let lastProgressAt = 0;

  function reportProgress(video, force) {
    const now = Date.now();
    if (!force && now - lastProgressAt < PROGRESS_INTERVAL_MS) return;
    const id = currentVideoId();
    if (!id) return;
    lastProgressAt = now;
    send({
      kind: 'progress',
      video_id: id,
      playing: !video.paused && !video.ended,
      position: seconds(video.currentTime),
      duration: seconds(video.duration),
      ad: isAd(),
    });
  }

  function reportFailure() {
    const id = currentVideoId();
    if (id) send({ kind: 'failed', video_id: id });
  }

  // Like the Swift app's autoplay recovery: call video.play() and retry until
  // the element plays or the attempts run out.
  function tryAutoplay(video) {
    if (video !== document.querySelector('video')) return;
    if (!video.paused) {
      autoplayPending = false;
      autoplayAttempts = 0;
      return;
    }
    if (hold || !autoplayPending) return;
    if (autoplayAttempts >= MAX_AUTOPLAY_ATTEMPTS) {
      autoplayPending = false;
      reportFailure();
      return;
    }
    autoplayAttempts += 1;
    Promise.resolve()
      .then(() => video.play())
      .catch(() => {});
    setTimeout(() => tryAutoplay(video), AUTOPLAY_RETRY_MS);
  }

  function hook(video) {
    hookedVideo = video;
    video.addEventListener('play', () => {
      if (hold) {
        video.pause();
        return;
      }
      reportProgress(video, true);
    });
    video.addEventListener('playing', () => reportProgress(video, true));
    video.addEventListener('pause', () => reportProgress(video, true));
    video.addEventListener('seeked', () => reportProgress(video, true));
    video.addEventListener('timeupdate', () => reportProgress(video, false));
    video.addEventListener('loadedmetadata', () => {
      reportProgress(video, true);
      tryAutoplay(video);
    });
    video.addEventListener('canplay', () => tryAutoplay(video));
    video.addEventListener('ended', () => {
      if (isAd()) return;
      const id = currentVideoId();
      if (!id) return;
      hold = true;
      send({ kind: 'ended', video_id: id });
    });
    video.addEventListener('error', reportFailure);
    if (video.readyState >= 1) tryAutoplay(video);
  }

  // The page creates and may replace its <video> element after load.
  setInterval(() => {
    const video = document.querySelector('video');
    if (video && video !== hookedVideo) hook(video);
  }, 500);

  Object.defineProperty(window, '__kasetPlayer', {
    value: Object.freeze({
      play() {
        hold = false;
        autoplayPending = true;
        autoplayAttempts = 0;
        const video = document.querySelector('video');
        if (video) tryAutoplay(video);
      },
      pause() {
        hold = true;
        autoplayPending = false;
        const video = document.querySelector('video');
        if (video && !video.paused) video.pause();
      },
      seek(position) {
        const video = document.querySelector('video');
        if (!video || !Number.isFinite(position)) return;
        video.currentTime = Math.max(0, position);
        reportProgress(video, true);
      },
    }),
    writable: false,
    configurable: false,
  });
})();
