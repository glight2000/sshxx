import { existsSync, statSync } from "node:fs";
import { dirname, join, resolve, sep } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

import { app, BrowserWindow, net, protocol, shell } from "electron";

const APP_SCHEME = "sshxx";

protocol.registerSchemesAsPrivileged([
  {
    scheme: APP_SCHEME,
    privileges: {
      standard: true,
      secure: true,
      supportFetchAPI: true,
      corsEnabled: true,
    },
  },
]);

function webRoot() {
  return app.isPackaged
    ? join(process.resourcesPath, "web")
    : resolve(dirname(fileURLToPath(import.meta.url)), "../../build");
}

function resolveWebFile(requestUrl) {
  const root = webRoot();
  const pathname = decodeURIComponent(new URL(requestUrl).pathname);
  const candidate = resolve(root, `.${pathname}`);
  if (candidate !== root && !candidate.startsWith(`${root}${sep}`)) return null;
  if (existsSync(candidate) && statSync(candidate).isFile()) return candidate;
  return join(root, "spa.html");
}

function createWindow() {
  const window = new BrowserWindow({
    width: 1280,
    height: 800,
    minWidth: 640,
    minHeight: 480,
    show: false,
    backgroundColor: "#09090b",
    webPreferences: {
      preload: join(dirname(fileURLToPath(import.meta.url)), "preload.cjs"),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
    },
  });

  window.webContents.setWindowOpenHandler(({ url }) => {
    if (url.startsWith("https://") || url.startsWith("http://")) {
      void shell.openExternal(url);
    }
    return { action: "deny" };
  });
  window.webContents.on("will-navigate", (event, url) => {
    if (!url.startsWith(`${APP_SCHEME}://`)) event.preventDefault();
  });
  window.once("ready-to-show", () => window.show());
  void window.loadURL(`${APP_SCHEME}://app/`);
}

app.whenReady().then(() => {
  protocol.handle(APP_SCHEME, (request) => {
    const file = resolveWebFile(request.url);
    return file
      ? net.fetch(pathToFileURL(file).toString())
      : new Response("Forbidden", { status: 403 });
  });
  createWindow();
  app.on("activate", () => {
    if (BrowserWindow.getAllWindows().length === 0) createWindow();
  });
});

app.on("window-all-closed", () => {
  if (process.platform !== "darwin") app.quit();
});
