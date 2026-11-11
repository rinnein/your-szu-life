export default function localNotificationPage(devUrl, initialize) {
  const current = new URL(window.location.href);
  if (current.username || current.password) return;

  const localUrls = ["tauri://localhost", "http://tauri.localhost", "https://tauri.localhost"];
  if (devUrl) localUrls.push(devUrl);
  const local = localUrls.some((value) => {
    const allowed = new URL(value);
    // Custom schemes have an opaque origin; compare protocol and host explicitly.
    return current.protocol === allowed.protocol && current.host === allowed.host;
  });
  if (local) initialize();
}
