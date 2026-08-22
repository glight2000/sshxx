export type FilePreviewKind =
  "none" | "image" | "audio" | "video" | "pdf" | "binary";

export function pathSeparator(path: string) {
  return path.includes("\\") ? "\\" : "/";
}

export function trimTrailingSeparators(path: string) {
  if (path === "/" || /^[A-Za-z]:[\\/]$/.test(path)) return path;
  return path.replace(/[\\/]+$/, "");
}

export function normalizedPath(path: string) {
  const normalized = trimTrailingSeparators(path).replace(/\\/g, "/");
  return /^[A-Za-z]:\//.test(normalized)
    ? normalized.toLowerCase()
    : normalized;
}

export function samePath(left: string, right: string) {
  return normalizedPath(left) === normalizedPath(right);
}

export function filesystemRoot(path: string) {
  const extendedDrive = path.match(/^\\\\\?\\([A-Za-z]:)\\/);
  if (extendedDrive) return `\\\\?\\${extendedDrive[1]}\\`;
  const drive = path.match(/^([A-Za-z]:)[\\/]/);
  if (drive) return `${drive[1]}${pathSeparator(path)}`;
  const unc = path.match(/^(\\\\[^\\]+\\[^\\]+)[\\]?/);
  if (unc) return `${unc[1]}\\`;
  return "/";
}

export function ancestorPaths(rootPath: string, targetPath: string) {
  const separator = pathSeparator(targetPath);
  const remainder = targetPath
    .slice(rootPath.length)
    .split(/[\\/]+/)
    .filter(Boolean);
  const paths = [rootPath];
  let cursor = rootPath;
  for (const part of remainder) {
    cursor = `${cursor.replace(/[\\/]+$/, "")}${separator}${part}`;
    paths.push(cursor);
  }
  return paths;
}

export function revealDirectoryPaths(expandedPaths: string[], path: string) {
  const result = [...expandedPaths];
  for (const ancestor of ancestorPaths(filesystemRoot(path), path)) {
    if (!result.some((candidate) => samePath(candidate, ancestor)))
      result.push(ancestor);
  }
  return result;
}

export function isPathInside(rootPath: string, path: string) {
  const root = normalizedPath(rootPath);
  const candidate = normalizedPath(path);
  return (
    candidate === root || candidate.startsWith(root === "/" ? root : `${root}/`)
  );
}

export function pathDepth(path: string) {
  return normalizedPath(path).split("/").filter(Boolean).length;
}

export function pathName(path: string) {
  return path.split(/[\\/]/).filter(Boolean).at(-1) || path;
}

export function childPath(directory: string, name: string) {
  const separator = pathSeparator(directory);
  return `${directory.replace(/[\\/]+$/, "")}${separator}${name}`;
}

export function parentPath(path: string) {
  const separator = pathSeparator(path);
  const trimmed = trimTrailingSeparators(path);
  if (/^[A-Za-z]:$/.test(trimmed)) return `${trimmed}${separator}`;
  const boundary = trimmed.lastIndexOf(separator);
  if (boundary === 2 && trimmed[1] === ":") return trimmed.slice(0, 3);
  if (boundary > 0) return trimmed.slice(0, boundary);
  return separator;
}

export function encodeBase64(bytes: Uint8Array) {
  let binary = "";
  const chunkSize = 32 << 10;
  for (let offset = 0; offset < bytes.length; offset += chunkSize) {
    binary += String.fromCharCode(
      ...bytes.subarray(offset, offset + chunkSize),
    );
  }
  return btoa(binary);
}

export function safeUploadPath(path: string) {
  const parts = path.split(/[\\/]+/).filter(Boolean);
  if (
    parts.length === 0 ||
    parts.some(
      (part) =>
        part === "." ||
        part === ".." ||
        part.length > 255 ||
        /[\0\u0000-\u001f]/.test(part),
    )
  )
    throw new Error(`Upload path “${path}” is invalid.`);
  return parts;
}

export function previewType(filename: string): FilePreviewKind {
  const extension = filename.split(".").at(-1)?.toLowerCase();
  if (
    [
      "png",
      "apng",
      "jpg",
      "jpeg",
      "jfif",
      "gif",
      "webp",
      "avif",
      "bmp",
      "svg",
      "ico",
    ].includes(extension ?? "")
  )
    return "image";
  if (
    ["mp3", "wav", "ogg", "oga", "opus", "flac", "aac", "m4a", "weba"].includes(
      extension ?? "",
    )
  )
    return "audio";
  if (["mp4", "m4v", "webm", "mov", "ogv"].includes(extension ?? ""))
    return "video";
  if (extension === "pdf") return "pdf";
  return "binary";
}

export function mimeType(filename: string) {
  const extension = filename.split(".").at(-1)?.toLowerCase();
  return (
    {
      png: "image/png",
      apng: "image/apng",
      jpg: "image/jpeg",
      jpeg: "image/jpeg",
      jfif: "image/jpeg",
      gif: "image/gif",
      webp: "image/webp",
      avif: "image/avif",
      bmp: "image/bmp",
      svg: "image/svg+xml",
      ico: "image/x-icon",
      mp3: "audio/mpeg",
      wav: "audio/wav",
      ogg: "audio/ogg",
      oga: "audio/ogg",
      opus: "audio/ogg; codecs=opus",
      flac: "audio/flac",
      aac: "audio/aac",
      m4a: "audio/mp4",
      weba: "audio/webm",
      mp4: "video/mp4",
      m4v: "video/mp4",
      webm: "video/webm",
      mov: "video/quicktime",
      ogv: "video/ogg",
      pdf: "application/pdf",
    }[extension ?? ""] ?? "application/octet-stream"
  );
}
