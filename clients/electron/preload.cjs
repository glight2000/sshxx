const { contextBridge } = require("electron");

contextBridge.exposeInMainWorld(
  "sshxxDesktop",
  Object.freeze({ runtime: "electron" }),
);
