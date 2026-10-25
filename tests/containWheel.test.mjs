import assert from "node:assert/strict";
import test from "node:test";
import {
  WheelInputClassifier,
  wheelDestination,
} from "../src/lib/wheelInput.ts";
import {
  containWheel,
  scrollWheel,
  installCanvasWheel,
} from "../src/lib/action/containWheel.ts";

test("native window menus contain wheel input and retain both scroll axes", () => {
  const previousElement = globalThis.Element;
  const previousStyle = globalThis.getComputedStyle;
  class Element extends EventTarget {
    parentElement = null;
    classList = { add() {}, remove() {} };
    closest() {
      return null;
    }
    scrollHeight = 100;
    clientHeight = 100;
    scrollWidth = 100;
    clientWidth = 100;
    scrollTop = 0;
    scrollLeft = 0;
    style = { overflowX: "auto", overflowY: "auto" };
    contains(element) {
      return element === this || element.parentElement === this;
    }
  }
  globalThis.Element = Element;
  globalThis.getComputedStyle = (element) => element.style;
  try {
    const root = new Element();
    const pane = new Element();
    pane.parentElement = root;
    let fallback = 0;
    const action = containWheel(root, () => fallback++);
    const wheel = (target, ctrlKey = false) => {
      const event = new Event("wheel", { cancelable: true });
      Object.defineProperties(event, {
        target: { value: target },
        ctrlKey: { value: ctrlKey },
        deltaX: { value: 100 },
        deltaY: { value: 100 },
      });
      root.dispatchEvent(event);
      return event.defaultPrevented;
    };
    assert.equal(wheel(pane), true, "empty pane cannot scroll the browser");
    assert.equal(fallback, 1);
    pane.scrollHeight = 400;
    assert.equal(
      wheel(pane),
      false,
      "native vertical scroll remains available",
    );
    pane.scrollTop = 300;
    assert.equal(wheel(pane), true, "bottom edge cannot chain to the page");
    pane.scrollTop = 0;
    pane.scrollHeight = 100;
    pane.scrollWidth = 400;
    assert.equal(
      wheel(pane),
      false,
      "native horizontal scroll remains available",
    );
    assert.equal(wheel(root, true), false, "Ctrl+wheel belongs to the canvas");
    assert.equal(fallback, 1);
    assert.equal(wheel(root), true, "window header uses content fallback");
    action.destroy();
    assert.equal(wheel(root), false);
    assert.equal(fallback, 2, "destroy removes both listeners");
  } finally {
    globalThis.Element = previousElement;
    globalThis.getComputedStyle = previousStyle;
  }
});

test("canvas capture routes to the focused surface, not the hovered window", () => {
  const previous = {
    Element: globalThis.Element,
    HTMLElement: globalThis.HTMLElement,
    document: globalThis.document,
  };
  class Element extends EventTarget {
    classList = { add() {}, remove() {} };
    parentElement = null;
    closest() {
      return null;
    }
    contains(target) {
      return target === this;
    }
    querySelector() {
      return this;
    }
  }
  globalThis.Element = globalThis.HTMLElement = Element;
  globalThis.document = { activeElement: null };
  try {
    const canvas = new Element(),
      focused = new Element(),
      hovered = new Element();
    let current = focused,
      scrolls = 0;
    const camera = [];
    const surface = containWheel(focused, () => scrolls++);
    let device = "mouse";
    const cleanup = installCanvasWheel(
      canvas,
      () => current,
      (_event, hasFocus, overFocus) =>
        wheelDestination(hasFocus, device, overFocus),
      (_event, mode) => camera.push(mode),
    );
    const wheel = (target, ctrlKey = false, timeStamp = 0) => {
      const event = new Event("wheel", { cancelable: true });
      Object.defineProperties(event, {
        target: { value: target },
        ctrlKey: { value: ctrlKey },
        timeStamp: { value: timeStamp },
      });
      canvas.dispatchEvent(event);
      return event.defaultPrevented;
    };
    assert.equal(wheel(hovered), true);
    assert.equal(scrolls, 1);
    assert.deepEqual(camera, []);
    assert.equal(
      wheel(canvas),
      true,
      "blank canvas still scrolls focused content",
    );
    assert.equal(scrolls, 2);
    assert.equal(
      wheel(hovered, true),
      false,
      "Ctrl+wheel is reserved for forced camera zoom",
    );
    const menu = new Element();
    menu.closest = () => menu;
    assert.equal(wheel(menu), false, "menus retain their native routing");
    device = "trackpad";
    wheel(focused, false, 200);
    wheel(canvas, false, 220);
    assert.equal(scrolls, 4, "a gesture starting inside stays inside");
    wheel(canvas, false, 600);
    wheel(focused, false, 620);
    assert.equal(scrolls, 4, "a gesture starting outside keeps panning");
    assert.deepEqual(camera, ["pan", "pan"]);
    camera.length = 0;
    current = null;
    wheel(hovered);
    assert.deepEqual(
      camera,
      ["pan"],
      "no focus routes over a window as well as blank space",
    );
    cleanup();
    surface.destroy();
    assert.equal(wheel(canvas), false);
  } finally {
    Object.assign(globalThis, previous);
  }
});

test("fallback scrolling preserves wheel units and both file-pane axes", () => {
  let result;
  const pane = { clientHeight: 200, scrollBy: (value) => (result = value) };
  scrollWheel(pane, { deltaX: 4, deltaY: 6, deltaMode: 0 });
  assert.deepEqual(result, { left: 4, top: 6 });
  scrollWheel(pane, { deltaX: 0, deltaY: 2, deltaMode: 1, shiftKey: true });
  assert.deepEqual(result, { left: 32, top: 0 });
  scrollWheel(pane, { deltaX: 0, deltaY: 1, deltaMode: 2 });
  assert.deepEqual(result, { left: 0, top: 200 });
  scrollWheel(null, {});
});

test("sparse trackpad samples preserve canvas ownership until a pause or explicit click", () => {
  const previous = {
    Element: globalThis.Element,
    HTMLElement: globalThis.HTMLElement,
    document: globalThis.document,
  };
  class Element extends EventTarget {
    classList = { add() {}, remove() {} };
    parentElement = null;
    closest() {
      return null;
    }
    contains(target) {
      return target === this;
    }
    querySelector() {
      return this;
    }
  }
  globalThis.Element = globalThis.HTMLElement = Element;
  globalThis.document = { activeElement: null };
  try {
    const canvas = new Element(),
      focused = new Element();
    const input = new WheelInputClassifier(),
      camera = [];
    let scrolls = 0,
      leakedEvents = 0;
    const surface = containWheel(focused, () => scrolls++);
    const cleanup = installCanvasWheel(
      canvas,
      () => focused,
      (event, hasFocus, overFocus) =>
        wheelDestination(hasFocus, input.classify(event, "auto"), overFocus),
      (_event, mode) => camera.push(mode),
    );
    canvas.addEventListener("wheel", () => leakedEvents++);
    const wheel = (target, timeStamp, deltaX, deltaY) => {
      const event = new Event("wheel", { cancelable: true });
      Object.defineProperties(event, {
        target: { value: target },
        timeStamp: { value: timeStamp },
        deltaX: { value: deltaX },
        deltaY: { value: deltaY },
        deltaMode: { value: 0 },
      });
      canvas.dispatchEvent(event);
      assert.equal(
        event.defaultPrevented,
        true,
        "canvas input cannot scroll the outer page",
      );
    };
    wheel(canvas, 0, 12, 1);
    wheel(canvas, 60, 8, 2);
    wheel(focused, 300, 0, 80);
    wheel(focused, 320, 3, 2);
    assert.deepEqual(camera, ["pan", "pan", "pan", "pan"]);
    assert.equal(scrolls, 0, "the moving window must not steal a sparse pan");
    wheel(focused, 700, 0, 0);
    assert.equal(scrolls, 0, "empty samples cannot pick a scroll target");
    wheel(focused, 850, 0, 120);
    wheel(canvas, 870, 4, 2);
    assert.equal(
      scrolls,
      2,
      "a new gesture inside the focused window still scrolls it",
    );
    wheel(canvas, 1300, 4, 2);
    canvas.dispatchEvent(new Event("pointerdown"));
    wheel(focused, 1320, 0, 2);
    assert.equal(
      scrolls,
      3,
      "a deliberate click ends the previous pan immediately",
    );
    assert.equal(leakedEvents, 0);
    cleanup();
    surface.destroy();
  } finally {
    Object.assign(globalThis, previous);
  }
});

test("a focused file window uses the clicked pane until editing selects another", () => {
  const previous = {
    Element: globalThis.Element,
    HTMLElement: globalThis.HTMLElement,
    document: globalThis.document,
    getComputedStyle: globalThis.getComputedStyle,
  };
  class Element extends EventTarget {
    parentElement = null;
    classList = { add() {}, remove() {} };
    style = { overflowX: "auto", overflowY: "auto" };
    closest() {
      return null;
    }
    contains(target) {
      return target === this || target?.parentElement === this;
    }
    querySelector() {
      return this;
    }
    scrollBy(value) {
      this.lastScroll = value;
    }
  }
  globalThis.Element = globalThis.HTMLElement = Element;
  globalThis.getComputedStyle = (e) => e.style;
  globalThis.document = { activeElement: null };
  try {
    const canvas = new Element(),
      file = new Element(),
      tree = new Element(),
      editor = new Element();
    tree.parentElement = editor.parentElement = file;
    const surface = containWheel(file);
    const cleanup = installCanvasWheel(
      canvas,
      () => file,
      () => "component",
      () => assert.fail("should not move camera"),
    );
    const pointer = new Event("pointerdown");
    Object.defineProperty(pointer, "target", { value: tree });
    file.dispatchEvent(pointer);
    const wheel = () => {
      const event = new Event("wheel", { cancelable: true });
      Object.defineProperties(event, {
        deltaX: { value: 30 },
        deltaY: { value: 80 },
        deltaMode: { value: 0 },
      });
      canvas.dispatchEvent(event);
    };
    wheel();
    assert.deepEqual(tree.lastScroll, { left: 30, top: 80 });
    assert.equal(editor.lastScroll, undefined);
    document.activeElement = editor;
    wheel();
    assert.deepEqual(editor.lastScroll, { left: 30, top: 80 });
    cleanup();
    surface.destroy();
  } finally {
    Object.assign(globalThis, previous);
  }
});
