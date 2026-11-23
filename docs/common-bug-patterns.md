# Common Bug Patterns

- **Stale session work.** Capture `Session::epoch()` before cookies or API
  requests. Apply results with epoch-aware methods so sign-out or sign-in can
  invalidate pending work.
- **Blocking work on the UI thread.** Send cookie and keyring I/O to
  `webview_session::blocking`. Route OS media control updates onto the main
  thread.
- **Unscoped query cache entries.** Use `queryKeys` and include the request
  inputs. Invalidate affected keys after a successful edit, and keep load
  errors with the screen that can retry them.
- **Reusing pagination cursors.** Keep continuations with the active request.
  Discard old pages after navigation, a session change, or queue replacement.
- **Confusing browse IDs and video IDs.** Follow the endpoint and parser's
  ID type. Only validated video IDs belong in playback URLs.
- **Guessing at lifecycle and timing.** Capture timestamps and relevant
  state transitions, reproduce, then rerun the same scenario after the fix.
  Remove temporary instrumentation before committing.
