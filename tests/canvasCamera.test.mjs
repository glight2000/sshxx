import assert from "node:assert/strict";
import test from "node:test";

import {
  canvasCameraCss,
  canvasWorldTransform,
  previewCanvasCamera,
  canvasToScreenPosition,
  canvasViewportAnchor,
  screenToCanvasPosition,
} from "../src/lib/canvasCamera.ts";

test("shared canvas camera maps world and screen coordinates consistently", () => {
  const anchor = canvasViewportAnchor(1440, 900, 378, 240);
  const center = [120, -80];
  const point = [438, 275];
  const zoom = 0.94317;
  const screen = canvasToScreenPosition(point, center, zoom, anchor);

  assert.deepEqual(screenToCanvasPosition(screen, center, zoom, anchor), point);
});

test("canvas viewport anchor preserves the established visual offset", () => {
  assert.deepEqual(canvasViewportAnchor(1440, 900, 378, 240), [342, 210]);
});

test("shared camera CSS updates the world and grid from one style value", () => {
  assert.equal(
    canvasCameraCss([10, -20], 0.5, 378, 240, 80),
    [
      "--canvas-world-x:calc(25vw - 194px)",
      "--canvas-world-y:calc(25vh - 110px)",
      "--canvas-world-zoom:0.5",
      "--canvas-grid-dot-size:0.5px",
      "--canvas-grid-step:40px",
    ].join(";"),
  );
});

test("touch previews change only the world transform and the childless grid", () => {
  const world = { style: { transform: "" } };
  const grid = { style: { cssText: "" } };
  for (const zoom of [0.35, 0.94317, 1, 2]) {
    previewCanvasCamera(world, grid, [10, -20], zoom, 378, 240, 80);
    assert.deepEqual(Object.keys(world.style), ["transform"]);
    assert.equal(
      world.style.transform,
      `translate3d(calc(${zoom * 50}vw - ${zoom * 388}px), calc(${zoom * 50}vh - ${zoom * 220}px), 0) scale(${zoom})`,
    );
    assert.equal(
      grid.style.cssText,
      canvasCameraCss([10, -20], zoom, 378, 240, 80),
    );
  }
});

test("page cameras stay independent during crossfade and active-page touch previews", () => {
  const oldCenter = [10, -20],
    newCenter = [-100, 80];
  const oldWorld = {
    style: { transform: canvasWorldTransform(oldCenter, 0.5, 378, 240) },
  };
  const newWorld = {
    style: { transform: canvasWorldTransform(newCenter, 1.5, 378, 240) },
  };
  const oldGrid = {
    style: { cssText: canvasCameraCss(oldCenter, 0.5, 378, 240, 80) },
  };
  const newGrid = {
    style: { cssText: canvasCameraCss(newCenter, 1.5, 378, 240, 80) },
  };
  const oldTransform = oldWorld.style.transform,
    oldGridCss = oldGrid.style.cssText;
  assert.notEqual(newWorld.style.transform, oldTransform);
  previewCanvasCamera(newWorld, newGrid, [20, 40], 2, 378, 240, 80);
  assert.equal(oldWorld.style.transform, oldTransform);
  assert.equal(oldGrid.style.cssText, oldGridCss);
  assert.equal(
    newWorld.style.transform,
    canvasWorldTransform([20, 40], 2, 378, 240),
  );
  // A quick return uses the old view, not the other page's most recent camera.
  previewCanvasCamera(oldWorld, oldGrid, oldCenter, 0.5, 378, 240, 80);
  assert.equal(oldWorld.style.transform, oldTransform);
  assert.equal(
    newWorld.style.transform,
    canvasWorldTransform([20, 40], 2, 378, 240),
  );
});
