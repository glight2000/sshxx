import assert from "node:assert/strict";
import { test } from "node:test";

import {
  ancestorPaths,
  childPath,
  filesystemRoot,
  mimeType,
  normalizedPath,
  parentPath,
  previewType,
  revealDirectoryPaths,
  safeUploadPath,
  samePath,
} from "../src/lib/filesystem.ts";

test("normalizes Unix and Windows paths without changing their native separator", () => {
  assert.equal(normalizedPath("C:\\Users\\Dev\\"), "c:/users/dev");
  assert.equal(filesystemRoot("C:\\Users\\Dev"), "C:\\");
  assert.equal(parentPath("C:\\Users\\Dev"), "C:\\Users");
  assert.equal(childPath("C:\\Users", "Dev"), "C:\\Users\\Dev");
  assert.equal(parentPath("/srv/sshxx"), "/srv");
  assert.equal(childPath("/srv", "sshxx"), "/srv/sshxx");
});

test("builds path ancestry and compares Windows paths case-insensitively", () => {
  assert.deepEqual(ancestorPaths("C:\\", "C:\\Users\\Dev"), [
    "C:\\",
    "C:\\Users",
    "C:\\Users\\Dev",
  ]);
  assert.equal(samePath("C:\\USERS\\Dev", "c:/users/dev/"), true);
});

test("reveals a directory and all of its ancestors without duplicate paths", () => {
  assert.deepEqual(revealDirectoryPaths(["/", "/srv"], "/srv/apps/sshxx"), [
    "/",
    "/srv",
    "/srv/apps",
    "/srv/apps/sshxx",
  ]);
  assert.deepEqual(
    revealDirectoryPaths(["C:\\", "C:\\Users"], "c:\\users\\Dev"),
    ["C:\\", "C:\\Users", "c:\\users\\Dev"],
  );
});

test("rejects unsafe relative upload paths", () => {
  assert.deepEqual(safeUploadPath("images/icons/logo.svg"), [
    "images",
    "icons",
    "logo.svg",
  ]);
  assert.throws(() => safeUploadPath("../secret"), /invalid/);
  assert.throws(() => safeUploadPath("folder/./file"), /invalid/);
  assert.throws(() => safeUploadPath(""), /invalid/);
});

test("classifies supported previews and MIME types", () => {
  assert.equal(previewType("photo.WEBP"), "image");
  assert.equal(previewType("photo.avif"), "image");
  assert.equal(previewType("recording.opus"), "audio");
  assert.equal(previewType("screen.m4v"), "video");
  assert.equal(previewType("manual.pdf"), "pdf");
  assert.equal(previewType("archive.zip"), "binary");
  assert.equal(mimeType("clip.mp4"), "video/mp4");
  assert.equal(mimeType("voice.aac"), "audio/aac");
  assert.equal(mimeType("archive.zip"), "application/octet-stream");
});
