import htmx from "./htmx.js";

export function startLiveReload() {
  window.startLiveReload = startLiveReload;
  const sse = new EventSource("/__livereload_status");
  let startTime = Date.now();
  sse.onerror = () => {
    startTime = Date.now();
    sse.close();
    pollForLive();
  };

  let liveReloadActive = true;
  async function pollForLive() {
    if (!liveReloadActive) {
      return;
    }

    try {
      let res = await fetch("/api/healthz");
      if (!res.ok) {
        throw new Error("server still down");
      }

      let endTime = Date.now();
      await htmx
        .ajax("GET", window.location.href, { target: "body" })
        .catch((e) => {
          console.error("AJAX reload failed", e);
          window.location.reload();
        });

      console.log(`Live reload took ${endTime - startTime}ms`);

      setTimeout(startLiveReload);
    } catch (e) {
      if (liveReloadActive) {
        setTimeout(pollForLive, 250);
      }
    }
  }

  window.stopLiveReload = () => {
    liveReloadActive = false;
    sse.close();
  };
}
