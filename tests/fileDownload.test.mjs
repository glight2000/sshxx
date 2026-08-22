import assert from "node:assert/strict";
import { test } from "node:test";
import { TextEncoder } from "node:util";

import {
  decodeDownloadContent,
  FILE_DOWNLOAD_LIMIT_BYTES,
  safeDownloadName,
} from "../src/lib/fileDownload.ts";

test("restores UTF-8 and binary download bytes", () => {
  assert.deepEqual(
    [...decodeDownloadContent("hello 世界", "utf8")],
    [...new TextEncoder().encode("hello 世界")],
  );
  assert.deepEqual(
    [...decodeDownloadContent("AAH+/w==", "base64")],
    [0, 1, 254, 255],
  );
});

test("restores BOM-marked UTF-16 downloads in their original byte order", () => {
  assert.deepEqual(
    [...decodeDownloadContent("A😀", "utf16le")],
    [0xff, 0xfe, 0x41, 0, 0x3d, 0xd8, 0, 0xde],
  );
  assert.deepEqual(
    [...decodeDownloadContent("A😀", "utf16be")],
    [0xfe, 0xff, 0, 0x41, 0xd8, 0x3d, 0xde, 0],
  );
});

test("sanitizes browser download names and exposes the transport limit", () => {
  assert.equal(safeDownloadName("report.txt"), "report.txt");
  assert.equal(safeDownloadName("folder/name\0.txt"), "folder_name_.txt");
  assert.equal(safeDownloadName("folder\\name.txt"), "folder_name.txt");
  assert.equal(safeDownloadName(".."), "download");
  assert.equal(FILE_DOWNLOAD_LIMIT_BYTES, 8 * 1024 * 1024);
});
