/** Mouse buttons assigned to blank-canvas selection and panning. */
export function canvasSelectionButton(swapped: boolean) {
  return swapped ? 2 : 0;
}

export function canvasPanButton(swapped: boolean) {
  return swapped ? 0 : 2;
}
