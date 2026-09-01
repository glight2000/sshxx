export const CUSTOM_COMPONENT_POINTER_MESSAGE =
  "sshxx-custom-pointer-v1" as const;
export const CUSTOM_COMPONENT_SET_URL_MESSAGE =
  "sshxx-custom-set-url-v1" as const;

export type CustomComponentPointer = {
  x: number;
  y: number;
  clicked: boolean;
};

/** Injected into trusted, user-authored srcdoc previews. URL previews may opt
 * into the same postMessage contract, but cannot be instrumented cross-origin. */
export const CUSTOM_COMPONENT_POINTER_BRIDGE = `<script>(()=>{const send=(event)=>{if(!event.isTrusted)return;parent.postMessage({type:"${CUSTOM_COMPONENT_POINTER_MESSAGE}",event:event.type==="click"?"click":"move",x:event.clientX,y:event.clientY},"*")};const setUrl=(url)=>parent.postMessage({type:"${CUSTOM_COMPONENT_SET_URL_MESSAGE}",url:String(url)},"*");Object.defineProperty(window,"sshxx",{value:Object.freeze({setUrl}),configurable:false,writable:false});window.addEventListener("pointermove",send,{capture:true,passive:true});window.addEventListener("click",send,{capture:true});})();</script>`;

export function customComponentRequestedUrl(value: unknown): string | null {
  if (!value || typeof value !== "object") return null;
  const message = value as Record<string, unknown>;
  return message.type === CUSTOM_COMPONENT_SET_URL_MESSAGE &&
    typeof message.url === "string"
    ? message.url
    : null;
}

export function mapCustomComponentPointer(
  value: unknown,
  frameWidth: number,
  frameHeight: number,
  componentWidth: number,
  componentHeight: number,
  titleHeight = 36,
): CustomComponentPointer | null {
  if (!value || typeof value !== "object") return null;
  const message = value as Record<string, unknown>;
  if (
    message.type !== CUSTOM_COMPONENT_POINTER_MESSAGE ||
    (message.event !== "move" && message.event !== "click") ||
    typeof message.x !== "number" ||
    typeof message.y !== "number" ||
    !Number.isFinite(message.x) ||
    !Number.isFinite(message.y) ||
    frameWidth <= 0 ||
    frameHeight <= 0 ||
    componentWidth <= 0 ||
    componentHeight <= titleHeight
  )
    return null;

  const contentHeight = componentHeight - titleHeight;
  return {
    x: Math.round(
      (Math.max(0, Math.min(message.x, frameWidth)) / frameWidth) *
        componentWidth,
    ),
    y: Math.round(
      titleHeight +
        (Math.max(0, Math.min(message.y, frameHeight)) / frameHeight) *
          contentHeight,
    ),
    clicked: message.event === "click",
  };
}
